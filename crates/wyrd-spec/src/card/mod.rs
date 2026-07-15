//! Card spec types, one struct per native [`crate::envelope::CardKind`].

pub mod agent;
pub mod artifact;
pub mod audit;
pub mod common;
pub mod data;
pub mod drift;
pub mod eval;
pub mod experiment;
pub mod field;
pub mod mcp;
pub mod model;
pub mod operator;
pub mod policy;
pub mod prompt;
pub mod service;
pub mod source;
pub mod trigger;
pub mod workflow;

pub use crate::ids::{ColumnName, FeatureName, QueryName, SplitName};
pub use common::{
    AgentInterface, CredentialRef, Governance, MetricEntry, NonSecretValue, ObservationHooks,
    ParameterValue, ProtocolProfile,
};
pub use data::{ColValue, Inequality};
pub use field::{Dim, FieldSpec};

#[cfg(test)]
mod data_field_tests {
    use std::collections::BTreeMap;

    use crate::card::{Dim, FieldSpec};
    use crate::ids::ColumnName;

    #[test]
    fn field_spec_constructor_builds_scalar_field() {
        let field = FieldSpec::new(ColumnName::new("amount").unwrap(), "float64");
        assert_eq!(field.name.as_str(), "amount");
        assert_eq!(field.dtype, "float64");
        assert!(field.shape.is_empty());
        assert!(!field.nullable);
        assert!(field.extra.is_empty());
    }

    #[test]
    fn field_spec_round_trips_with_shape_and_extra() {
        let field = FieldSpec {
            name: ColumnName::new("embedding").unwrap(),
            dtype: "float32".to_string(),
            shape: vec![Dim::Fixed(768), Dim::Dynamic(Some("batch".to_string()))],
            nullable: true,
            extra: BTreeMap::from([("source".to_string(), "test".to_string())]),
        };

        let json = serde_json::to_string(&field).unwrap();
        let round_tripped: FieldSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(round_tripped, field);
    }

    #[test]
    fn card_column_names_accept_one_character_tokens() {
        assert!(ColumnName::new("x").is_ok());
        assert!(ColumnName::new("X").is_err());
    }
}

#[cfg(test)]
mod data_methods_tests {
    use std::collections::HashMap;

    use crate::card::data::{
        ColValue, DataInterface, DataSchema, DataSpec, DataSplit, DataStats, PandasMeta,
        ParquetCompression, SplitStrategy,
    };
    use crate::card::{FieldSpec, Inequality};
    use crate::envelope::CardKind;
    use crate::ids::{CardName, ColumnName, SpaceName, SplitName};
    use crate::reference::CardRef;
    use wyrd_semver::VersionBlock;

    fn col(name: &str) -> ColumnName {
        ColumnName::new(name).unwrap()
    }

    fn split(name: &str) -> SplitName {
        SplitName::new(name).unwrap()
    }

    fn schema() -> DataSchema {
        DataSchema::new(vec![
            FieldSpec::new(col("feature"), "int64"),
            FieldSpec::new(col("target"), "bool"),
        ])
    }

    fn stats() -> DataStats {
        DataStats {
            row_count: Some(3),
            col_count: Some(2),
            byte_count: 12,
            sha256: "b".repeat(64),
        }
    }

    fn card_ref() -> CardRef {
        CardRef {
            kind: CardKind::Artifact,
            name: CardName::new("artifact").unwrap(),
            version: VersionBlock::parse("1.0.0").unwrap(),
            space: SpaceName::new("default").expect("static space is valid"),
            uid: None,
        }
    }

    fn interface() -> DataInterface {
        DataInterface::Pandas(PandasMeta {
            framework_version: "2.2.2".to_string(),
            compression: ParquetCompression::Snappy,
        })
    }

    #[test]
    fn data_spec_helpers_expose_interface_refs_stats_targets_and_splits() {
        let split_ref = card_ref();
        let train_split = DataSplit::materialized(split("train"), split_ref.clone());
        let artifact = card_ref();
        let spec = DataSpec {
            interface: interface(),
            schema: schema(),
            card_refs: vec![artifact.clone()],
            splits: HashMap::from([(split("train"), train_split)]),
            target_columns: vec![col("target")],
            sql: None,
            stats: stats(),
        };
        spec.validate().unwrap();

        assert_eq!(spec.interface_kind(), "Pandas");
        assert!(spec.is_tabular());
        assert_eq!(spec.card_refs().collect::<Vec<_>>(), vec![&artifact]);
        assert_eq!(spec.target_columns().next().unwrap().as_str(), "target");
        assert_eq!(spec.stats().byte_count, 12);
        assert!(spec.split(&split("train")).is_some());
        assert_eq!(
            spec.materialized_split_refs().collect::<Vec<_>>(),
            vec![&split_ref]
        );
    }

    #[test]
    fn data_interface_helpers_are_stable() {
        let interface = interface();
        assert_eq!(interface.kind(), "Pandas");
        assert!(interface.requires_schema_columns());
        assert!(!interface.requires_sql_logic());
        assert_eq!(interface.default_media_type(), "application/x-parquet");
        assert_eq!(interface.default_extension(), "parquet");
        assert_eq!(interface.loader_family(), "pandas");
        assert!(interface.manifest_ref().is_none());
    }

    #[test]
    fn data_schema_helpers_preserve_order() {
        let schema = schema();
        assert!(!schema.is_empty());
        assert!(schema.contains_column(&col("feature")));
        assert_eq!(schema.column(&col("target")).unwrap().dtype, "bool");
        assert_eq!(
            schema
                .column_names()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["feature", "target"]
        );
    }

    #[test]
    fn split_helpers_expose_strategy_details() {
        let materialized = DataSplit::materialized(split("train"), card_ref());
        assert!(materialized.card_ref().is_some());
        assert_eq!(materialized.strategy.kind(), "Materialized");

        let column = DataSplit::column(
            split("test"),
            col("feature"),
            Inequality::Gt,
            ColValue::Int(3),
        );
        assert_eq!(column.referenced_column().unwrap().as_str(), "feature");
        assert_eq!(column.strategy.kind(), "Column");

        let range = DataSplit::index_range(split("empty"), 4, 4);
        assert!(matches!(
            range.strategy,
            SplitStrategy::IndexRange { start: 4, stop: 4 }
        ));

        let indices = DataSplit::indices(split("rows"), vec![0, 2]);
        assert!(matches!(indices.strategy, SplitStrategy::Indices(_)));
    }
}

#[cfg(test)]
mod data_roundtrip_tests {
    use std::collections::{BTreeMap, HashMap};

    use crate::card::data::{
        ArrowFormat, ArrowMeta, ColValue, ColorMode, CustomDataMeta, DataInterface, DataSchema,
        DataSpec, DataSplit, DataStats, HuggingfaceMeta, ImageFormat, ImageMeta, JsonlCompression,
        JsonlMeta, NumpyFormat, NumpyMeta, PandasMeta, ParquetCompression, ParquetMeta, PolarsMeta,
        SplitStrategy, SqlLogic, SqlMeta, TextMeta, TorchMeta, TorchSaveFormat,
    };
    use crate::card::{FieldSpec, Inequality};
    use crate::envelope::CardKind;
    use crate::ids::{CardName, ColumnName, QueryName, SpaceName, SplitName};
    use crate::reference::CardRef;
    use wyrd_semver::VersionBlock;

    fn col(name: &str) -> ColumnName {
        ColumnName::new(name).unwrap()
    }

    fn split(name: &str) -> SplitName {
        SplitName::new(name).unwrap()
    }

    fn query(name: &str) -> QueryName {
        QueryName::new(name).unwrap()
    }

    fn card_ref(name: &str) -> CardRef {
        CardRef {
            kind: CardKind::Artifact,
            name: CardName::new(name).unwrap(),
            version: VersionBlock::parse("1.0.0").unwrap(),
            space: SpaceName::new("default").expect("static space is valid"),
            uid: None,
        }
    }

    fn schema() -> DataSchema {
        DataSchema::new(vec![
            FieldSpec::new(col("feature"), "int64"),
            FieldSpec::new(col("target"), "bool"),
        ])
    }

    fn stats() -> DataStats {
        DataStats {
            row_count: Some(10),
            col_count: Some(2),
            byte_count: 10,
            sha256: "c".repeat(64),
        }
    }

    fn spec(interface: DataInterface) -> DataSpec {
        let sql = matches!(interface, DataInterface::Sql(_)).then(|| SqlLogic {
            queries: HashMap::from([(query("main"), "select * from data".to_string())]),
            default_query: Some(query("main")),
        });
        let schema = if interface.requires_schema_columns() {
            schema()
        } else {
            DataSchema::empty()
        };
        let spec = DataSpec {
            interface,
            schema,
            card_refs: Vec::new(),
            splits: HashMap::new(),
            target_columns: Vec::new(),
            sql,
            stats: stats(),
        };
        spec.validate().unwrap();
        spec
    }

    fn interfaces() -> Vec<DataInterface> {
        vec![
            DataInterface::Pandas(PandasMeta {
                framework_version: "2.2.2".to_string(),
                compression: ParquetCompression::Snappy,
            }),
            DataInterface::Polars(PolarsMeta {
                framework_version: "1.0.0".to_string(),
                compression: ParquetCompression::Zstd,
            }),
            DataInterface::Arrow(ArrowMeta {
                framework_version: "16.0.0".to_string(),
                format: ArrowFormat::Ipc,
            }),
            DataInterface::Parquet(ParquetMeta {
                compression: ParquetCompression::Gzip,
                row_group_size: Some(1024),
            }),
            DataInterface::Numpy(NumpyMeta {
                dtype: "float32".to_string(),
                shape: vec![2, 3],
                format: NumpyFormat::Npy,
            }),
            DataInterface::Torch(TorchMeta {
                framework_version: "2.4.0".to_string(),
                save_format: TorchSaveFormat::Safetensors,
            }),
            DataInterface::Sql(SqlMeta {
                dialect: "postgres".to_string(),
                connection_hint: Some("warehouse".to_string()),
            }),
            DataInterface::Jsonl(JsonlMeta {
                compression: JsonlCompression::Gzip,
                lines_per_file: Some(1000),
            }),
            DataInterface::Image(ImageMeta {
                format: ImageFormat::Mixed,
                manifest_ref: Some(card_ref("imagemanifest")),
                color_mode: ColorMode::Rgb,
            }),
            DataInterface::Text(TextMeta {
                encoding: "utf-8".to_string(),
                manifest_ref: Some(card_ref("textmanifest")),
            }),
            DataInterface::Huggingface(HuggingfaceMeta {
                dataset_id: "acme/data".to_string(),
                revision: Some("abcdef0".to_string()),
                split: Some("train".to_string()),
                config: None,
            }),
            DataInterface::Custom(CustomDataMeta {
                loader_module: "acme.loader".to_string(),
                loader_class: "Loader".to_string(),
                extra: BTreeMap::from([("mode".to_string(), "test".to_string())]),
            }),
        ]
    }

    #[test]
    fn every_interface_variant_round_trips_json_and_yaml() {
        for interface in interfaces() {
            let spec = spec(interface);
            let json = serde_json::to_string_pretty(&spec).unwrap();
            let from_json: DataSpec = serde_json::from_str(&json).unwrap();
            assert_eq!(from_json, spec);

            let yaml = serde_yaml::to_string(&spec).unwrap();
            let from_yaml: DataSpec = serde_yaml::from_str(&yaml).unwrap();
            assert_eq!(from_yaml, spec);
        }
    }

    #[test]
    fn every_split_strategy_variant_round_trips_json_and_yaml() {
        let splits = vec![
            DataSplit::materialized(split("materialized"), card_ref("splitartifact")),
            DataSplit::column(
                split("column"),
                col("feature"),
                Inequality::Le,
                ColValue::Int(5),
            ),
            DataSplit::index_range(split("range"), 0, 10),
            DataSplit::indices(split("indices"), vec![1, 3, 5]),
        ];

        for split in splits {
            let json = serde_json::to_string_pretty(&split).unwrap();
            let from_json: DataSplit = serde_json::from_str(&json).unwrap();
            assert_eq!(from_json, split);

            let yaml = serde_yaml::to_string(&split).unwrap();
            let from_yaml: DataSplit = serde_yaml::from_str(&yaml).unwrap();
            assert_eq!(from_yaml, split);
        }
    }

    #[test]
    fn split_strategy_kind_strings_are_stable() {
        assert_eq!(
            SplitStrategy::Materialized(card_ref("artifact")).kind(),
            "Materialized"
        );
        assert_eq!(
            SplitStrategy::Column {
                name: col("feature"),
                op: Inequality::Eq,
                value: ColValue::Bool(true),
            }
            .kind(),
            "Column"
        );
        assert_eq!(
            SplitStrategy::IndexRange { start: 0, stop: 1 }.kind(),
            "IndexRange"
        );
        assert_eq!(SplitStrategy::Indices(vec![0]).kind(), "Indices");
    }
}

#[cfg(test)]
mod data_validation_tests {
    use std::collections::HashMap;

    use crate::card::data::validate::{DataCardError, validate_data_spec};
    use crate::card::data::{
        ArrowFormat, ArrowMeta, ColValue, DataInterface, DataSchema, DataSpec, DataSplit,
        DataStats, HuggingfaceMeta, JsonlCompression, JsonlMeta, NumpyFormat, NumpyMeta,
        PandasMeta, ParquetCompression, ParquetMeta, PolarsMeta, SqlLogic, SqlMeta,
    };
    use crate::card::{FieldSpec, Inequality};
    use crate::ids::{ColumnName, QueryName, SplitName};

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
        let error: crate::error::WyrdError =
            DataCardError::TargetColumnUnknown("target".to_string()).into();
        assert_eq!(error.code(), "WYRD_DATA_400_TARGET_COLUMN_UNKNOWN");

        let error: crate::error::WyrdError =
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
}

#[cfg(test)]
mod model_methods_tests {
    use std::collections::BTreeMap;

    use crate::card::field::FieldSpec;
    use crate::card::model::{
        CatboostMeta, CustomMeta, HuggingFaceTask, HuggingfaceMeta, LightgbmMeta, LightningMeta,
        ModelCardError, ModelInterface, ModelSignature, ModelSpec, SampleInput, SampleInputKind,
        SklearnMeta, TaskType, TensorflowMeta, TfSaveFormat, TorchMeta, TorchSaveFormat,
        XgboostMeta,
    };
    use crate::envelope::CardKind;
    use crate::ids::{CardName, ColumnName, SpaceName};
    use crate::reference::CardRef;
    use wyrd_semver::VersionBlock;

    fn col(name: &str) -> ColumnName {
        ColumnName::new(name).unwrap()
    }

    fn field(name: &str, dtype: &str) -> FieldSpec {
        FieldSpec::new(col(name), dtype)
    }

    fn model_ref(name: &str) -> CardRef {
        CardRef {
            kind: CardKind::Artifact,
            name: CardName::new(name).unwrap(),
            version: VersionBlock::parse("1.0.0").unwrap(),
            space: SpaceName::new("default").expect("static space is valid"),
            uid: None,
        }
    }

    fn signature() -> ModelSignature {
        ModelSignature::new(vec![field("x", "float64")], vec![field("y", "bool")])
    }

    fn sklearn_interface() -> ModelInterface {
        ModelInterface::Sklearn(SklearnMeta {
            framework_version: "1.4.2".to_string(),
            model_subtype: Some("RandomForestClassifier".to_string()),
        })
    }

    fn huggingface_interface(task: HuggingFaceTask) -> ModelInterface {
        ModelInterface::Huggingface(HuggingfaceMeta {
            framework_version: "4.40.0".to_string(),
            model_subtype: None,
            hf_task: task,
            repo_id: Some("acme/model".to_string()),
            revision: Some("abcdef0".to_string()),
        })
    }

    fn custom_interface() -> ModelInterface {
        ModelInterface::Custom(CustomMeta {
            framework_version: "0.1.0".to_string(),
            model_subtype: None,
            loader_module: "mypkg.loaders".to_string(),
            loader_class: "GraphLoader".to_string(),
            extra: BTreeMap::from([("graph_backend".to_string(), "dgl".to_string())]),
        })
    }

    #[test]
    fn model_spec_new_runs_validation() {
        let err = ModelSpec::new(
            sklearn_interface(),
            TaskType::BinaryClassification,
            ModelSignature::new(Vec::new(), vec![field("y", "bool")]),
            None,
            Vec::new(),
        )
        .unwrap_err();
        assert_eq!(err, ModelCardError::EmptyInputs);

        let spec = ModelSpec::new(
            sklearn_interface(),
            TaskType::BinaryClassification,
            signature(),
            None,
            Vec::new(),
        )
        .unwrap();
        assert_eq!(spec.task_type, TaskType::BinaryClassification);
    }

    #[test]
    fn model_spec_validate_is_idempotent() {
        let spec = ModelSpec::new(
            sklearn_interface(),
            TaskType::BinaryClassification,
            signature(),
            None,
            Vec::new(),
        )
        .unwrap();

        spec.validate().expect("first validation");
        spec.validate().expect("second validation");
    }

    #[test]
    fn model_spec_interface_kind_matches_variant_tag() {
        let spec = ModelSpec::new(
            sklearn_interface(),
            TaskType::BinaryClassification,
            signature(),
            None,
            Vec::new(),
        )
        .unwrap();
        assert_eq!(spec.interface_kind(), "Sklearn");
    }

    #[test]
    fn model_spec_is_generation_only_for_generation_task() {
        let mut spec = ModelSpec::new(
            sklearn_interface(),
            TaskType::BinaryClassification,
            signature(),
            None,
            Vec::new(),
        )
        .unwrap();
        assert!(!spec.is_generation());

        spec.task_type = TaskType::Generation;
        assert!(spec.is_generation());
    }

    #[test]
    fn model_spec_card_refs_iterates_declared_refs() {
        let refs = vec![
            model_ref("model"),
            model_ref("tokenizer"),
            model_ref("sample"),
        ];
        let spec = ModelSpec::new(
            huggingface_interface(HuggingFaceTask::TextClassification),
            TaskType::MultiClassClassification,
            signature(),
            Some(SampleInput::new(SampleInputKind::Pandas)),
            refs.clone(),
        )
        .unwrap();

        assert_eq!(
            spec.card_refs().collect::<Vec<_>>(),
            refs.iter().collect::<Vec<_>>()
        );
    }

    #[test]
    fn model_spec_card_refs_empty_for_local_spec_without_refs() {
        let spec = ModelSpec::new(
            sklearn_interface(),
            TaskType::Regression,
            signature(),
            None,
            Vec::new(),
        )
        .unwrap();
        assert_eq!(spec.card_refs().count(), 0);
    }

    #[test]
    fn model_interface_kind_loader_media_and_extension_are_stable() {
        let cases = vec![
            (
                sklearn_interface(),
                "Sklearn",
                "sklearn",
                "application/x-joblib",
                "joblib",
            ),
            (
                ModelInterface::Xgboost(XgboostMeta {
                    framework_version: "2.0.3".to_string(),
                    model_subtype: None,
                }),
                "Xgboost",
                "xgboost",
                "application/x-joblib",
                "joblib",
            ),
            (
                ModelInterface::Lightgbm(LightgbmMeta {
                    framework_version: "4.3.0".to_string(),
                    model_subtype: None,
                }),
                "Lightgbm",
                "lightgbm",
                "application/x-joblib",
                "joblib",
            ),
            (
                ModelInterface::Catboost(CatboostMeta {
                    framework_version: "1.2.5".to_string(),
                    model_subtype: None,
                }),
                "Catboost",
                "catboost",
                "application/x-joblib",
                "joblib",
            ),
            (
                ModelInterface::Torch(TorchMeta {
                    framework_version: "2.3.0".to_string(),
                    model_subtype: None,
                    save_format: TorchSaveFormat::Safetensors,
                }),
                "Torch",
                "torch",
                "application/vnd.safetensors",
                "safetensors",
            ),
            (
                ModelInterface::Torch(TorchMeta {
                    framework_version: "2.3.0".to_string(),
                    model_subtype: None,
                    save_format: TorchSaveFormat::Pickle,
                }),
                "Torch",
                "torch",
                "application/vnd.safetensors",
                "pt",
            ),
            (
                ModelInterface::Lightning(LightningMeta {
                    framework_version: "2.2.4".to_string(),
                    model_subtype: None,
                }),
                "Lightning",
                "pytorch-lightning",
                "application/x-pytorch-ckpt",
                "ckpt",
            ),
            (
                ModelInterface::Tensorflow(TensorflowMeta {
                    framework_version: "2.16.1".to_string(),
                    model_subtype: None,
                    save_format: TfSaveFormat::Keras,
                }),
                "Tensorflow",
                "tensorflow",
                "application/vnd.keras+zip",
                "keras",
            ),
            (
                ModelInterface::Tensorflow(TensorflowMeta {
                    framework_version: "2.16.1".to_string(),
                    model_subtype: None,
                    save_format: TfSaveFormat::SavedModel,
                }),
                "Tensorflow",
                "tensorflow",
                "application/vnd.keras+zip",
                "savedmodel",
            ),
            (
                huggingface_interface(HuggingFaceTask::Other),
                "Huggingface",
                "huggingface-transformers",
                "application/vnd.huggingface+bundle",
                "huggingface",
            ),
            (
                custom_interface(),
                "Custom",
                "custom-python-loader",
                "application/octet-stream",
                "bin",
            ),
        ];

        for (interface, kind, loader_family, media_type, extension) in cases {
            assert_eq!(interface.kind(), kind);
            assert_eq!(interface.loader_family(), loader_family);
            assert_eq!(interface.default_media_type(), media_type);
            assert_eq!(interface.default_extension(), extension);
        }
    }

    #[test]
    fn model_signature_new_preserves_fields_without_validation() {
        let signature = ModelSignature::new(Vec::new(), Vec::new());
        assert!(signature.inputs().is_empty());
        assert!(signature.outputs().is_empty());
        assert_eq!(signature.validate(), Err(ModelCardError::EmptyInputs));
    }

    #[test]
    fn model_signature_from_fields_validates_and_preserves_order() {
        let err = ModelSignature::from_fields(
            vec![field("x", "float64"), field("x", "int64")],
            vec![field("y", "bool")],
        )
        .unwrap_err();
        assert_eq!(
            err,
            ModelCardError::DuplicateFieldName {
                side: "inputs",
                name: "x".to_string()
            }
        );

        let signature = ModelSignature::from_fields(
            vec![field("x", "float64"), field("z", "int64")],
            vec![field("y", "bool")],
        )
        .unwrap();
        assert_eq!(signature.inputs()[0].name.as_str(), "x");
        assert_eq!(signature.inputs()[1].name.as_str(), "z");
        assert_eq!(signature.outputs()[0].name.as_str(), "y");
        assert!(signature.validate().is_ok());
    }

    #[test]
    fn sample_input_new_and_kind_are_stable() {
        let sample = SampleInput::new(SampleInputKind::Pandas);
        assert_eq!(sample.kind(), SampleInputKind::Pandas);

        let none = SampleInput::new(SampleInputKind::None);
        assert_eq!(none.kind(), SampleInputKind::None);
    }
}

#[cfg(test)]
mod model_roundtrip_tests {
    use std::collections::BTreeMap;

    use crate::card::field::FieldSpec;
    use crate::card::model::{
        CatboostMeta, CustomMeta, HuggingFaceTask, HuggingfaceMeta, LightgbmMeta, LightningMeta,
        ModelInterface, ModelSignature, ModelSpec, SampleInput, SampleInputKind, SklearnMeta,
        TaskType, TensorflowMeta, TfSaveFormat, TorchMeta, TorchSaveFormat, XgboostMeta,
    };
    use crate::envelope::CardKind;
    use crate::ids::{CardName, ColumnName, SpaceName};
    use crate::reference::CardRef;
    use serde::Serialize;
    use serde::de::DeserializeOwned;
    use wyrd_semver::VersionBlock;

    fn col(name: &str) -> ColumnName {
        ColumnName::new(name).unwrap()
    }

    fn field(name: &str, dtype: &str) -> FieldSpec {
        FieldSpec::new(col(name), dtype)
    }

    fn model_ref(name: &str) -> CardRef {
        CardRef {
            kind: CardKind::Artifact,
            name: CardName::new(name).unwrap(),
            version: VersionBlock::parse("1.0.0").unwrap(),
            space: SpaceName::new("default").expect("static space is valid"),
            uid: None,
        }
    }

    fn signature() -> ModelSignature {
        ModelSignature::new(vec![field("x", "float64")], vec![field("y", "bool")])
    }

    fn spec(interface: ModelInterface) -> ModelSpec {
        ModelSpec {
            interface,
            task_type: TaskType::Other,
            signature: signature(),
            sample_input: Some(SampleInput::new(SampleInputKind::Dict)),
            card_refs: vec![model_ref("model")],
        }
    }

    fn assert_json_yaml_roundtrip<T>(value: &T)
    where
        T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let json = serde_json::to_string_pretty(value).unwrap();
        let from_json: T = serde_json::from_str(&json).unwrap();
        assert_eq!(&from_json, value);

        let yaml = serde_yaml::to_string(value).unwrap();
        let from_yaml: T = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(&from_yaml, value);
    }

    fn round_trip_spec(interface: ModelInterface) {
        assert_json_yaml_roundtrip(&spec(interface));
    }

    fn huggingface_interface(task: HuggingFaceTask) -> ModelInterface {
        ModelInterface::Huggingface(HuggingfaceMeta {
            framework_version: "4.40.0".to_string(),
            model_subtype: None,
            hf_task: task,
            repo_id: Some("acme/model".to_string()),
            revision: Some("abcdef0".to_string()),
        })
    }

    #[test]
    fn every_model_interface_variant_round_trips_json_and_yaml() {
        let mut extra = BTreeMap::new();
        extra.insert("graph_backend".to_string(), "dgl".to_string());

        let interfaces = vec![
            ModelInterface::Sklearn(SklearnMeta {
                framework_version: "1.4.2".to_string(),
                model_subtype: Some("RandomForestClassifier".to_string()),
            }),
            ModelInterface::Xgboost(XgboostMeta {
                framework_version: "2.0.3".to_string(),
                model_subtype: Some("XGBClassifier".to_string()),
            }),
            ModelInterface::Lightgbm(LightgbmMeta {
                framework_version: "4.3.0".to_string(),
                model_subtype: None,
            }),
            ModelInterface::Catboost(CatboostMeta {
                framework_version: "1.2.5".to_string(),
                model_subtype: None,
            }),
            ModelInterface::Torch(TorchMeta {
                framework_version: "2.3.0".to_string(),
                model_subtype: Some("MyNet".to_string()),
                save_format: TorchSaveFormat::Safetensors,
            }),
            ModelInterface::Torch(TorchMeta {
                framework_version: "2.3.0".to_string(),
                model_subtype: None,
                save_format: TorchSaveFormat::Pickle,
            }),
            ModelInterface::Lightning(LightningMeta {
                framework_version: "2.2.4".to_string(),
                model_subtype: None,
            }),
            ModelInterface::Tensorflow(TensorflowMeta {
                framework_version: "2.16.1".to_string(),
                model_subtype: None,
                save_format: TfSaveFormat::Keras,
            }),
            ModelInterface::Tensorflow(TensorflowMeta {
                framework_version: "2.16.1".to_string(),
                model_subtype: None,
                save_format: TfSaveFormat::SavedModel,
            }),
            huggingface_interface(HuggingFaceTask::TextClassification),
            ModelInterface::Custom(CustomMeta {
                framework_version: "0.1.0".to_string(),
                model_subtype: None,
                loader_module: "mypkg.loaders".to_string(),
                loader_class: "GraphLoader".to_string(),
                extra,
            }),
        ];

        for interface in interfaces {
            round_trip_spec(interface);
        }
    }

    #[test]
    fn every_huggingface_task_round_trips_in_huggingface_meta() {
        let tasks = [
            HuggingFaceTask::TextClassification,
            HuggingFaceTask::TokenClassification,
            HuggingFaceTask::QuestionAnswering,
            HuggingFaceTask::Summarization,
            HuggingFaceTask::Translation,
            HuggingFaceTask::TextGeneration,
            HuggingFaceTask::FillMask,
            HuggingFaceTask::ZeroShotClassification,
            HuggingFaceTask::ImageClassification,
            HuggingFaceTask::ObjectDetection,
            HuggingFaceTask::ImageSegmentation,
            HuggingFaceTask::ImageToText,
            HuggingFaceTask::ImageToImage,
            HuggingFaceTask::TextToImage,
            HuggingFaceTask::DepthEstimation,
            HuggingFaceTask::AudioClassification,
            HuggingFaceTask::AutomaticSpeechRecognition,
            HuggingFaceTask::AudioToAudio,
            HuggingFaceTask::TextToSpeech,
            HuggingFaceTask::TabularClassification,
            HuggingFaceTask::TabularRegression,
            HuggingFaceTask::FeatureExtraction,
            HuggingFaceTask::SentenceSimilarity,
            HuggingFaceTask::Conversational,
            HuggingFaceTask::DocumentQuestionAnswering,
            HuggingFaceTask::VisualQuestionAnswering,
            HuggingFaceTask::TableQuestionAnswering,
            HuggingFaceTask::Embedding,
            HuggingFaceTask::MultipleChoice,
            HuggingFaceTask::Other,
        ];

        for task in tasks {
            round_trip_spec(huggingface_interface(task));
        }
    }

    #[test]
    fn every_sample_input_kind_round_trips_json_and_yaml() {
        let kinds = [
            SampleInputKind::Pandas,
            SampleInputKind::Polars,
            SampleInputKind::Arrow,
            SampleInputKind::Numpy,
            SampleInputKind::Torch,
            SampleInputKind::Tf,
            SampleInputKind::Dict,
            SampleInputKind::List,
            SampleInputKind::Tuple,
            SampleInputKind::Str,
            SampleInputKind::None,
        ];

        for kind in kinds {
            assert_json_yaml_roundtrip(&SampleInput::new(kind));
            let mut spec = spec(huggingface_interface(HuggingFaceTask::TextGeneration));
            spec.sample_input = Some(SampleInput::new(kind));
            assert_json_yaml_roundtrip(&spec);
        }
    }

    #[test]
    fn every_task_type_round_trips_json_and_yaml() {
        let task_types = [
            TaskType::BinaryClassification,
            TaskType::MultiClassClassification,
            TaskType::Regression,
            TaskType::Clustering,
            TaskType::AnomalyDetection,
            TaskType::Forecasting,
            TaskType::Generation,
            TaskType::Other,
        ];

        for task_type in task_types {
            assert_json_yaml_roundtrip(&task_type);
            let mut spec = spec(huggingface_interface(HuggingFaceTask::TextGeneration));
            spec.task_type = task_type;
            assert_json_yaml_roundtrip(&spec);
        }
    }

    #[test]
    fn save_format_enums_round_trip_json_and_yaml() {
        for save_format in [TorchSaveFormat::Safetensors, TorchSaveFormat::Pickle] {
            assert_json_yaml_roundtrip(&save_format);
        }
        for save_format in [TfSaveFormat::Keras, TfSaveFormat::SavedModel] {
            assert_json_yaml_roundtrip(&save_format);
        }
    }
}

#[cfg(test)]
mod model_schema_drift_tests {
    use std::fs;

    use crate::card::model::{
        HuggingFaceTask, ModelInterface, ModelSignature, ModelSpec, SampleInput, SampleInputKind,
        TaskType, TfSaveFormat, TorchSaveFormat,
    };
    use schemars::schema_for;

    fn snapshot_path(file_name: &str) -> String {
        format!("{}/tests/schemas/{file_name}", env!("CARGO_MANIFEST_DIR"))
    }

    fn assert_schema_matches_snapshot<T: schemars::JsonSchema>(file_name: &str) {
        let mut schema = schema_for!(T);
        schema.meta_schema = Some("https://json-schema.org/draft/2020-12/schema".to_string());
        let actual = format!("{}\n", serde_json::to_string_pretty(&schema).unwrap());
        let path = snapshot_path(file_name);
        let expected =
            fs::read_to_string(&path).unwrap_or_else(|error| panic!("missing {path}: {error}"));
        assert_eq!(actual, expected, "schema drift in {file_name}");
    }

    #[test]
    fn model_spec_schema_matches_snapshot() {
        assert_schema_matches_snapshot::<ModelSpec>("model_spec.json");
    }

    #[test]
    fn model_interface_schema_matches_snapshot() {
        assert_schema_matches_snapshot::<ModelInterface>("model_interface.json");
    }

    #[test]
    fn task_type_schema_matches_snapshot() {
        assert_schema_matches_snapshot::<TaskType>("task_type.json");
    }

    #[test]
    fn model_signature_schema_matches_snapshot() {
        assert_schema_matches_snapshot::<ModelSignature>("model_signature.json");
    }

    #[test]
    fn sample_input_schemas_match_snapshots() {
        assert_schema_matches_snapshot::<SampleInput>("sample_input.json");
        assert_schema_matches_snapshot::<SampleInputKind>("sample_input_kind.json");
    }

    #[test]
    fn torch_save_format_schema_matches_snapshot() {
        assert_schema_matches_snapshot::<TorchSaveFormat>("torch_save_format.json");
    }

    #[test]
    fn tf_save_format_schema_matches_snapshot() {
        assert_schema_matches_snapshot::<TfSaveFormat>("tf_save_format.json");
    }

    #[test]
    fn hugging_face_task_schema_matches_snapshot() {
        assert_schema_matches_snapshot::<HuggingFaceTask>("hugging_face_task.json");
    }
}

#[cfg(test)]
mod model_validation_tests {
    use std::collections::BTreeMap;

    use crate::card::field::{Dim, FieldSpec, is_canonical_dtype};
    use crate::card::model::validate::{ModelCardError, validate_model_spec};
    use crate::card::model::{
        CatboostMeta, CustomMeta, HuggingFaceTask, HuggingfaceMeta, LightgbmMeta, LightningMeta,
        ModelInterface, ModelSignature, ModelSpec, SampleInput, SampleInputKind, SklearnMeta,
        TaskType, TensorflowMeta, TfSaveFormat, TorchMeta, TorchSaveFormat, XgboostMeta,
    };
    use crate::error::WyrdError;
    use crate::ids::ColumnName;
    use serde_json::json;

    fn col(name: &str) -> ColumnName {
        ColumnName::new(name).unwrap()
    }

    fn field(name: &str, dtype: &str) -> FieldSpec {
        FieldSpec::new(col(name), dtype)
    }

    fn valid_signature() -> ModelSignature {
        ModelSignature::new(
            vec![field("input", "float32")],
            vec![field("output", "float32")],
        )
    }

    fn valid_interface() -> ModelInterface {
        ModelInterface::Sklearn(SklearnMeta {
            framework_version: "1.4.0".to_string(),
            model_subtype: None,
        })
    }

    fn valid_spec() -> ModelSpec {
        ModelSpec {
            interface: valid_interface(),
            task_type: TaskType::Regression,
            signature: valid_signature(),
            sample_input: None,
            card_refs: Vec::new(),
        }
    }

    fn empty_framework_interfaces() -> Vec<(&'static str, ModelInterface)> {
        vec![
            (
                "Sklearn",
                ModelInterface::Sklearn(SklearnMeta {
                    framework_version: String::new(),
                    model_subtype: None,
                }),
            ),
            (
                "Xgboost",
                ModelInterface::Xgboost(XgboostMeta {
                    framework_version: String::new(),
                    model_subtype: None,
                }),
            ),
            (
                "Lightgbm",
                ModelInterface::Lightgbm(LightgbmMeta {
                    framework_version: String::new(),
                    model_subtype: None,
                }),
            ),
            (
                "Catboost",
                ModelInterface::Catboost(CatboostMeta {
                    framework_version: String::new(),
                    model_subtype: None,
                }),
            ),
            (
                "Torch",
                ModelInterface::Torch(TorchMeta {
                    framework_version: String::new(),
                    model_subtype: None,
                    save_format: TorchSaveFormat::Safetensors,
                }),
            ),
            (
                "Lightning",
                ModelInterface::Lightning(LightningMeta {
                    framework_version: String::new(),
                    model_subtype: None,
                }),
            ),
            (
                "Tensorflow",
                ModelInterface::Tensorflow(TensorflowMeta {
                    framework_version: String::new(),
                    model_subtype: None,
                    save_format: TfSaveFormat::Keras,
                }),
            ),
            (
                "Huggingface",
                ModelInterface::Huggingface(HuggingfaceMeta {
                    framework_version: String::new(),
                    model_subtype: None,
                    hf_task: HuggingFaceTask::TextGeneration,
                    repo_id: None,
                    revision: None,
                }),
            ),
            (
                "Custom",
                ModelInterface::Custom(CustomMeta {
                    framework_version: String::new(),
                    model_subtype: None,
                    loader_module: "pkg.loader".to_string(),
                    loader_class: "Loader".to_string(),
                    extra: BTreeMap::new(),
                }),
            ),
        ]
    }

    #[test]
    fn signature_inputs_empty_rejected() {
        let mut spec = valid_spec();
        spec.signature.inputs.clear();
        assert_eq!(validate_model_spec(&spec), Err(ModelCardError::EmptyInputs));
    }

    #[test]
    fn signature_outputs_empty_rejected() {
        let mut spec = valid_spec();
        spec.signature.outputs.clear();
        assert_eq!(
            validate_model_spec(&spec),
            Err(ModelCardError::EmptyOutputs)
        );
    }

    #[test]
    fn duplicate_input_field_rejected() {
        let mut spec = valid_spec();
        spec.signature.inputs.push(field("input", "float64"));
        assert_eq!(
            validate_model_spec(&spec),
            Err(ModelCardError::DuplicateFieldName {
                side: "inputs",
                name: "input".to_string()
            })
        );
    }

    #[test]
    fn duplicate_output_field_rejected() {
        let mut spec = valid_spec();
        spec.signature.outputs.push(field("output", "float64"));
        assert_eq!(
            validate_model_spec(&spec),
            Err(ModelCardError::DuplicateFieldName {
                side: "outputs",
                name: "output".to_string()
            })
        );
    }

    #[test]
    fn same_name_allowed_across_sides_accepted() {
        let spec = ModelSpec {
            signature: ModelSignature::from_fields(
                vec![field("x", "int64")],
                vec![field("x", "int64")],
            )
            .unwrap(),
            ..valid_spec()
        };
        assert!(spec.validate().is_ok());
    }

    #[test]
    fn framework_version_empty_rejected_for_all_variants() {
        for (expected, interface) in empty_framework_interfaces() {
            let spec = ModelSpec {
                interface,
                ..valid_spec()
            };
            assert_eq!(
                validate_model_spec(&spec),
                Err(ModelCardError::EmptyFrameworkVersion { variant: expected })
            );
        }
    }

    #[test]
    fn canonical_dtype_accepted() {
        for dtype in [
            "bool",
            "int8",
            "uint64",
            "float16",
            "large_utf8",
            "large_binary",
            "date64",
            "time32[ms]",
            "time64[ns]",
            "timestamp[us, tz=UTC]",
            "duration[ns]",
            "decimal128(10, 2)",
            "decimal256(76, 0)",
            "list<int64>",
            "large_list<utf8>",
            "fixed_size_list<float32, 4>",
            "struct<a:int64,b:list<float32>>",
            "dictionary<int32, utf8>",
        ] {
            assert!(is_canonical_dtype(dtype), "{dtype} should be accepted");
        }
        let spec = ModelSpec {
            signature: ModelSignature::new(
                vec![field("x", "list<int64>")],
                vec![field("y", "bool")],
            ),
            ..valid_spec()
        };
        assert!(validate_model_spec(&spec).is_ok());
    }

    #[test]
    fn unknown_dtype_rejected() {
        for dtype in [
            "",
            "string",
            "bfloat16",
            "timestamp[day]",
            "dictionary<float32, utf8>",
        ] {
            assert!(!is_canonical_dtype(dtype), "{dtype} should be rejected");
        }
        let spec = ModelSpec {
            signature: ModelSignature::new(
                vec![field("x", "bfloat16")],
                vec![field("y", "float32")],
            ),
            ..valid_spec()
        };
        assert_eq!(
            validate_model_spec(&spec),
            Err(ModelCardError::DtypeNormalizeFailed {
                dtype: "bfloat16".to_string()
            })
        );
    }

    #[test]
    fn shape_fixed_zero_rejected() {
        let mut input = field("x", "float32");
        input.shape = vec![Dim::Fixed(0)];
        let spec = ModelSpec {
            signature: ModelSignature::new(vec![input], vec![field("y", "float32")]),
            ..valid_spec()
        };
        assert_eq!(
            validate_model_spec(&spec),
            Err(ModelCardError::ShapeFixedNonPositive {
                field: "x".to_string(),
                value: 0
            })
        );
    }

    #[test]
    fn shape_fixed_negative_rejected() {
        let mut input = field("x", "float32");
        input.shape = vec![Dim::Fixed(-1)];
        let spec = ModelSpec {
            signature: ModelSignature::new(vec![input], vec![field("y", "float32")]),
            ..valid_spec()
        };
        assert!(matches!(
            validate_model_spec(&spec),
            Err(ModelCardError::ShapeFixedNonPositive { value: -1, .. })
        ));
    }

    #[test]
    fn shape_dynamic_and_scalar_accepted() {
        let mut dynamic = field("x", "float32");
        dynamic.shape = vec![Dim::Dynamic(Some("batch".to_string()))];
        let spec = ModelSpec {
            signature: ModelSignature::new(vec![dynamic], vec![field("y", "float32")]),
            ..valid_spec()
        };
        assert!(validate_model_spec(&spec).is_ok());

        let spec = valid_spec();
        assert!(validate_model_spec(&spec).is_ok());
    }

    #[test]
    fn hf_revision_and_repo_id_validation() {
        for revision in ["abcdef0", &"a".repeat(40)] {
            let spec = ModelSpec {
                interface: ModelInterface::Huggingface(HuggingfaceMeta {
                    framework_version: "4.41.0".to_string(),
                    model_subtype: None,
                    hf_task: HuggingFaceTask::TextGeneration,
                    repo_id: Some("acme/model".to_string()),
                    revision: Some(revision.to_string()),
                }),
                ..valid_spec()
            };
            assert!(validate_model_spec(&spec).is_ok());
        }

        for revision in ["abcdef", "ABCDEF0", &"a".repeat(41)] {
            let spec = ModelSpec {
                interface: ModelInterface::Huggingface(HuggingfaceMeta {
                    framework_version: "4.41.0".to_string(),
                    model_subtype: None,
                    hf_task: HuggingFaceTask::TextGeneration,
                    repo_id: Some("acme/model".to_string()),
                    revision: Some(revision.to_string()),
                }),
                ..valid_spec()
            };
            assert_eq!(
                validate_model_spec(&spec),
                Err(ModelCardError::HuggingfaceRevisionInvalid {
                    value: revision.to_string()
                })
            );
        }

        let spec = ModelSpec {
            interface: ModelInterface::Huggingface(HuggingfaceMeta {
                framework_version: "4.41.0".to_string(),
                model_subtype: None,
                hf_task: HuggingFaceTask::TextGeneration,
                repo_id: Some(" ".to_string()),
                revision: None,
            }),
            ..valid_spec()
        };
        assert_eq!(
            validate_model_spec(&spec),
            Err(ModelCardError::HuggingfaceRepoIdEmpty)
        );
    }

    #[test]
    fn hf_task_other_accepted() {
        let spec = ModelSpec {
            interface: ModelInterface::Huggingface(HuggingfaceMeta {
                framework_version: "4.41.0".to_string(),
                model_subtype: None,
                hf_task: HuggingFaceTask::Other,
                repo_id: None,
                revision: None,
            }),
            ..valid_spec()
        };
        assert!(validate_model_spec(&spec).is_ok());
    }

    #[test]
    fn custom_loader_module_and_class_validation() {
        let mut spec = ModelSpec {
            interface: ModelInterface::Custom(CustomMeta {
                framework_version: "1.0.0".to_string(),
                model_subtype: None,
                loader_module: "pkg.loader".to_string(),
                loader_class: "Loader".to_string(),
                extra: BTreeMap::new(),
            }),
            ..valid_spec()
        };
        assert!(validate_model_spec(&spec).is_ok());

        spec.interface = ModelInterface::Custom(CustomMeta {
            framework_version: "1.0.0".to_string(),
            model_subtype: None,
            loader_module: " ".to_string(),
            loader_class: "Loader".to_string(),
            extra: BTreeMap::new(),
        });
        assert_eq!(
            validate_model_spec(&spec),
            Err(ModelCardError::CustomLoaderInvalid {
                field: "loader_module"
            })
        );

        spec.interface = ModelInterface::Custom(CustomMeta {
            framework_version: "1.0.0".to_string(),
            model_subtype: None,
            loader_module: "pkg.loader".to_string(),
            loader_class: String::new(),
            extra: BTreeMap::new(),
        });
        assert_eq!(
            validate_model_spec(&spec),
            Err(ModelCardError::CustomLoaderInvalid {
                field: "loader_class"
            })
        );
    }

    #[test]
    fn model_helpers_are_stable() {
        let interface = ModelInterface::Torch(TorchMeta {
            framework_version: "2.4.0".to_string(),
            model_subtype: Some("Module".to_string()),
            save_format: TorchSaveFormat::Pickle,
        });
        assert_eq!(interface.kind(), "Torch");
        assert_eq!(interface.loader_family(), "torch");
        assert_eq!(
            interface.default_media_type(),
            "application/vnd.safetensors"
        );
        assert_eq!(interface.default_extension(), "pt");

        let sample = SampleInput::new(SampleInputKind::Dict);
        assert_eq!(sample.kind(), SampleInputKind::Dict);

        let spec = ModelSpec::new(
            interface,
            TaskType::Generation,
            valid_signature(),
            Some(sample),
            Vec::new(),
        )
        .unwrap();
        assert_eq!(spec.interface_kind(), "Torch");
        assert!(spec.is_generation());
        assert_eq!(spec.signature.inputs().len(), 1);
        assert_eq!(spec.signature.outputs().len(), 1);
        assert_eq!(spec.card_refs().count(), 0);
    }

    #[test]
    fn modelcard_error_maps_to_public_wyrd_error_codes_and_details() {
        let error: WyrdError = ModelCardError::EmptyInputs.into();
        assert_eq!(error.code(), "WYRD_MODEL_400_MISSING_SIGNATURE");
        assert_eq!(
            error.as_problem_json()["details"],
            json!({ "side": "inputs" })
        );

        let error: WyrdError = ModelCardError::EmptyOutputs.into();
        assert_eq!(error.code(), "WYRD_MODEL_400_MISSING_SIGNATURE");
        assert_eq!(
            error.as_problem_json()["details"],
            json!({ "side": "outputs" })
        );

        let error: WyrdError = ModelCardError::DuplicateFieldName {
            side: "outputs",
            name: "score".to_string(),
        }
        .into();
        assert_eq!(error.code(), "WYRD_MODEL_400_VALIDATION");
        assert_eq!(error.as_problem_json()["details"]["name"], "score");

        let error: WyrdError = ModelCardError::EmptyFrameworkVersion { variant: "Sklearn" }.into();
        assert_eq!(error.code(), "WYRD_MODEL_400_VALIDATION");
        assert_eq!(
            error.as_problem_json()["details"]["interface_kind"],
            "Sklearn"
        );

        let error: WyrdError = ModelCardError::DtypeNormalizeFailed {
            dtype: "object".to_string(),
        }
        .into();
        assert_eq!(error.code(), "WYRD_MODEL_400_DTYPE_NORMALIZE_FAILED");
        assert_eq!(error.as_problem_json()["details"]["dtype"], "object");

        let error: WyrdError = ModelCardError::ShapeFixedNonPositive {
            field: "x".to_string(),
            value: 0,
        }
        .into();
        assert_eq!(error.code(), "WYRD_MODEL_400_SHAPE_INVALID");
        assert_eq!(error.as_problem_json()["details"]["field"], "x");

        let error: WyrdError = ModelCardError::HuggingfaceRevisionInvalid {
            value: "ABCDEF0".to_string(),
        }
        .into();
        assert_eq!(error.code(), "WYRD_MODEL_400_HF_REVISION_INVALID");

        let error: WyrdError = ModelCardError::HuggingfaceRepoIdEmpty.into();
        assert_eq!(error.code(), "WYRD_MODEL_400_VALIDATION");
        assert_eq!(error.as_problem_json()["details"]["field"], "repo_id");

        let error: WyrdError = ModelCardError::HuggingfaceTaskMissing.into();
        assert_eq!(error.code(), "WYRD_MODEL_400_HF_TASK_MISSING");

        let error: WyrdError = ModelCardError::CustomLoaderInvalid {
            field: "loader_class",
        }
        .into();
        assert_eq!(error.code(), "WYRD_MODEL_400_CUSTOM_LOADER_INVALID");
        assert_eq!(error.as_problem_json()["details"]["field"], "loader_class");
    }
}

#[cfg(test)]
mod drift_validation_tests {
    use std::collections::BTreeMap;

    use crate::card::drift::{
        CustomProfile, DriftCondition, DriftMethod, DriftProfile, DriftSignal, DriftSpec,
        DriftValidationError, PsiBinningStrategy, PsiProfile, PsiThreshold, SpcAlertThreshold,
        SpcProfile, SpcWecoRule,
    };
    use crate::envelope::CardKind;
    use crate::error::WyrdError;
    use crate::ids::{CardName, FeatureName, SpaceName};
    use crate::reference::CardRef;
    use wyrd_semver::VersionBlock;

    fn card_ref(kind: CardKind, name: &str, version: &str) -> CardRef {
        CardRef {
            kind,
            name: CardName::new(name).expect("valid card name"),
            version: VersionBlock::parse(version).expect("valid version"),
            space: SpaceName::new("default").expect("valid space"),
            uid: None,
        }
    }

    fn data_ref(name: &str) -> CardRef {
        card_ref(CardKind::Data, name, "1.0.0")
    }

    fn model_ref(name: &str) -> CardRef {
        card_ref(CardKind::Model, name, "1.0.0")
    }

    fn psi_profile() -> PsiProfile {
        PsiProfile {
            binning_strategy: PsiBinningStrategy::EqualWidth { n_bins: 10 },
            categorical_features: vec![],
            threshold: PsiThreshold::ChiSquare { alpha: 0.05 },
        }
    }

    fn spc_profile() -> SpcProfile {
        SpcProfile {
            sample_size: 0,
            weco_rule: SpcWecoRule::default(),
            alert_threshold: SpcAlertThreshold::Zone4,
        }
    }

    fn custom_profile() -> CustomProfile {
        CustomProfile {
            metric_name: "latency".to_string(),
            baseline_value: 100.0,
            alert_threshold: 10.0,
        }
    }

    fn distribution_signal() -> DriftSignal {
        DriftSignal::Distribution {
            baseline_ref: data_ref("baseline-data"),
            features: vec![FeatureName::new("feature_a").expect("valid feature")],
        }
    }

    fn metric_signal() -> DriftSignal {
        DriftSignal::Metric {
            name: "latency".to_string(),
        }
    }

    fn external_signal() -> DriftSignal {
        DriftSignal::External {
            source_ref: data_ref("source-data"),
        }
    }

    #[test]
    fn happy_path_psi_distribution_statistical() {
        let spec = DriftSpec::new(
            DriftMethod::Psi,
            model_ref("subject-model"),
            distribution_signal(),
            DriftCondition::Statistical,
            Some(DriftProfile::Psi(psi_profile())),
            None,
            BTreeMap::new(),
        );

        assert!(spec.is_ok(), "{spec:?}");
    }

    #[test]
    fn happy_path_external_method_with_comparator_condition() {
        let spec = DriftSpec::new(
            DriftMethod::External,
            model_ref("subject-model"),
            external_signal(),
            DriftCondition::Above { limit: 1.0 },
            None,
            None,
            BTreeMap::new(),
        );

        assert!(spec.is_ok(), "{spec:?}");
    }

    #[test]
    fn rejects_subject_ref_not_in_allowed_set() {
        let err = DriftSpec::new(
            DriftMethod::Psi,
            card_ref(CardKind::Trigger, "subject-trigger", "1.0.0"),
            distribution_signal(),
            DriftCondition::Statistical,
            Some(DriftProfile::Psi(psi_profile())),
            None,
            BTreeMap::new(),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            DriftValidationError::InvalidSubjectKind { .. }
        ));
    }

    #[test]
    fn rejects_psi_with_metric_signal() {
        let err = DriftSpec::new(
            DriftMethod::Psi,
            model_ref("subject-model"),
            metric_signal(),
            DriftCondition::Statistical,
            Some(DriftProfile::Psi(psi_profile())),
            None,
            BTreeMap::new(),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            DriftValidationError::SignalMethodMismatch { .. }
        ));
    }

    #[test]
    fn rejects_custom_with_external_signal() {
        let err = DriftSpec::new(
            DriftMethod::Custom,
            model_ref("subject-model"),
            external_signal(),
            DriftCondition::Statistical,
            Some(DriftProfile::Custom(custom_profile())),
            None,
            BTreeMap::new(),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            DriftValidationError::SignalMethodMismatch { .. }
        ));
    }

    #[test]
    fn rejects_distribution_with_non_data_baseline_ref() {
        let signal = DriftSignal::Distribution {
            baseline_ref: model_ref("baseline-model"),
            features: vec![FeatureName::new("feature_a").expect("valid feature")],
        };

        let err = DriftSpec::new(
            DriftMethod::Psi,
            model_ref("subject-model"),
            signal,
            DriftCondition::Statistical,
            Some(DriftProfile::Psi(psi_profile())),
            None,
            BTreeMap::new(),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            DriftValidationError::BaselineRefMustBeData { .. }
        ));
    }

    #[test]
    fn rejects_distribution_with_no_features() {
        let signal = DriftSignal::Distribution {
            baseline_ref: data_ref("baseline-data"),
            features: vec![],
        };

        let err = DriftSpec::new(
            DriftMethod::Psi,
            model_ref("subject-model"),
            signal,
            DriftCondition::Statistical,
            Some(DriftProfile::Psi(psi_profile())),
            None,
            BTreeMap::new(),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            DriftValidationError::DistributionMissingFeatures
        ));
    }

    #[test]
    fn rejects_distribution_with_duplicate_features() {
        let signal = DriftSignal::Distribution {
            baseline_ref: data_ref("baseline-data"),
            features: vec![
                FeatureName::new("duplicate").expect("valid feature"),
                FeatureName::new("duplicate").expect("valid feature"),
            ],
        };

        let err = DriftSpec::new(
            DriftMethod::Psi,
            model_ref("subject-model"),
            signal,
            DriftCondition::Statistical,
            Some(DriftProfile::Psi(psi_profile())),
            None,
            BTreeMap::new(),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            DriftValidationError::DistributionDuplicateFeatures { .. }
        ));
    }

    #[test]
    fn rejects_eval_score_with_non_eval_ref() {
        let signal = DriftSignal::EvalScore {
            eval_ref: data_ref("not-eval"),
        };

        let err = DriftSpec::new(
            DriftMethod::Spc,
            model_ref("subject-model"),
            signal,
            DriftCondition::Statistical,
            Some(DriftProfile::Spc(spc_profile())),
            None,
            BTreeMap::new(),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            DriftValidationError::EvalRefMustBeEval { .. }
        ));
    }

    #[test]
    fn rejects_external_source_ref_with_drift_kind() {
        let signal = DriftSignal::External {
            source_ref: card_ref(CardKind::Drift, "bad-source", "1.0.0"),
        };

        let err = DriftSpec::new(
            DriftMethod::External,
            model_ref("subject-model"),
            signal,
            DriftCondition::Above { limit: 1.0 },
            None,
            None,
            BTreeMap::new(),
        )
        .unwrap_err();

        assert!(matches!(err, DriftValidationError::SourceRefInvalidKind));
    }

    #[test]
    fn rejects_profile_required_for_psi() {
        let err = DriftSpec::new(
            DriftMethod::Psi,
            model_ref("subject-model"),
            distribution_signal(),
            DriftCondition::Above { limit: 1.0 },
            None,
            None,
            BTreeMap::new(),
        )
        .unwrap_err();

        assert!(matches!(err, DriftValidationError::ProfileRequired { .. }));
    }

    #[test]
    fn rejects_profile_variant_that_does_not_match_method() {
        let err = DriftSpec::new(
            DriftMethod::Psi,
            model_ref("subject-model"),
            distribution_signal(),
            DriftCondition::Statistical,
            Some(DriftProfile::Spc(spc_profile())),
            None,
            BTreeMap::new(),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            DriftValidationError::ProfileMethodMismatch { .. }
        ));
    }

    #[test]
    fn rejects_profile_for_external_method() {
        let err = DriftSpec::new(
            DriftMethod::External,
            model_ref("subject-model"),
            external_signal(),
            DriftCondition::Above { limit: 1.0 },
            Some(DriftProfile::Psi(psi_profile())),
            None,
            BTreeMap::new(),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            DriftValidationError::ProfileForbiddenForExternal
        ));
    }

    #[test]
    fn rejects_external_method_with_statistical_condition() {
        let err = DriftSpec::new(
            DriftMethod::External,
            model_ref("subject-model"),
            external_signal(),
            DriftCondition::Statistical,
            None,
            None,
            BTreeMap::new(),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            DriftValidationError::StatisticalRequiresProfile
        ));
    }

    #[test]
    fn rejects_above_with_non_finite_limit() {
        let err = DriftSpec::new(
            DriftMethod::Custom,
            model_ref("subject-model"),
            metric_signal(),
            DriftCondition::Above { limit: f64::NAN },
            Some(DriftProfile::Custom(custom_profile())),
            None,
            BTreeMap::new(),
        )
        .unwrap_err();

        assert!(matches!(err, DriftValidationError::NonFiniteLimit { .. }));
    }

    #[test]
    fn rejects_outside_with_inverted_bounds() {
        let err = DriftSpec::new(
            DriftMethod::Custom,
            model_ref("subject-model"),
            metric_signal(),
            DriftCondition::Outside {
                lower: 10.0,
                upper: 1.0,
            },
            Some(DriftProfile::Custom(custom_profile())),
            None,
            BTreeMap::new(),
        )
        .unwrap_err();

        assert!(matches!(err, DriftValidationError::OutsideBoundsInverted));
    }

    #[test]
    fn rejects_psi_alpha_out_of_range() {
        let mut profile = psi_profile();
        profile.threshold = PsiThreshold::Normal { alpha: 1.0 };

        let err = DriftSpec::new(
            DriftMethod::Psi,
            model_ref("subject-model"),
            distribution_signal(),
            DriftCondition::Statistical,
            Some(DriftProfile::Psi(profile)),
            None,
            BTreeMap::new(),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            DriftValidationError::PsiAlphaOutOfRange { .. }
        ));
    }

    #[test]
    fn rejects_psi_fixed_threshold_when_not_positive() {
        let mut profile = psi_profile();
        profile.threshold = PsiThreshold::Fixed { value: 0.0 };

        let err = DriftSpec::new(
            DriftMethod::Psi,
            model_ref("subject-model"),
            distribution_signal(),
            DriftCondition::Statistical,
            Some(DriftProfile::Psi(profile)),
            None,
            BTreeMap::new(),
        )
        .unwrap_err();

        assert!(matches!(err, DriftValidationError::PsiFixedInvalid));
    }

    #[test]
    fn rejects_psi_bin_count_out_of_range() {
        let mut profile = psi_profile();
        profile.binning_strategy = PsiBinningStrategy::Quantile { n_bins: 1 };

        let err = DriftSpec::new(
            DriftMethod::Psi,
            model_ref("subject-model"),
            distribution_signal(),
            DriftCondition::Statistical,
            Some(DriftProfile::Psi(profile)),
            None,
            BTreeMap::new(),
        )
        .unwrap_err();

        assert!(matches!(err, DriftValidationError::PsiBinCountOutOfRange));
    }

    #[test]
    fn rejects_spc_sample_size_one() {
        let mut profile = spc_profile();
        profile.sample_size = 1;

        let err = DriftSpec::new(
            DriftMethod::Spc,
            model_ref("subject-model"),
            metric_signal(),
            DriftCondition::Statistical,
            Some(DriftProfile::Spc(profile)),
            None,
            BTreeMap::new(),
        )
        .unwrap_err();

        assert!(matches!(err, DriftValidationError::SpcSampleSizeOutOfRange));
    }

    #[test]
    fn rejects_malformed_spc_weco_rule() {
        let mut profile = spc_profile();
        profile.weco_rule.rule_string = "8 16 0 8 2 4 1 1".to_string();

        let err = DriftSpec::new(
            DriftMethod::Spc,
            model_ref("subject-model"),
            metric_signal(),
            DriftCondition::Statistical,
            Some(DriftProfile::Spc(profile)),
            None,
            BTreeMap::new(),
        )
        .unwrap_err();

        assert!(matches!(err, DriftValidationError::SpcWecoMalformed));
    }

    #[test]
    fn rejects_custom_profile_with_empty_metric_name() {
        let mut profile = custom_profile();
        profile.metric_name = "   ".to_string();

        let err = DriftSpec::new(
            DriftMethod::Custom,
            model_ref("subject-model"),
            metric_signal(),
            DriftCondition::Statistical,
            Some(DriftProfile::Custom(profile)),
            None,
            BTreeMap::new(),
        )
        .unwrap_err();

        assert!(matches!(err, DriftValidationError::CustomMetricNameEmpty));
    }

    #[test]
    fn rejects_custom_profile_with_invalid_numbers() {
        let mut profile = custom_profile();
        profile.alert_threshold = -1.0;

        let err = DriftSpec::new(
            DriftMethod::Custom,
            model_ref("subject-model"),
            metric_signal(),
            DriftCondition::Statistical,
            Some(DriftProfile::Custom(profile)),
            None,
            BTreeMap::new(),
        )
        .unwrap_err();

        assert!(matches!(err, DriftValidationError::CustomInvalidNumber));
    }

    #[test]
    fn signal_method_mismatch_routes_to_dedicated_catalog_code() {
        let err: WyrdError = DriftValidationError::SignalMethodMismatch {
            signal: "Metric".to_string(),
            method: "Psi".to_string(),
        }
        .into();

        assert_eq!(err.code(), "WYRD_DRIFT_400_SIGNAL_METHOD_MISMATCH");
        assert_eq!(err.status(), 400);
        let details = match &err {
            WyrdError::DriftSignalMethodMismatch { details, .. } => details,
            other => panic!("wrong variant: {other:?}"),
        };
        assert_eq!(
            details.get("signal").and_then(|value| value.as_str()),
            Some("Metric")
        );
        assert_eq!(
            details.get("method").and_then(|value| value.as_str()),
            Some("Psi")
        );
    }

    #[test]
    fn profile_required_routes_to_dedicated_catalog_code() {
        let err: WyrdError = DriftValidationError::ProfileRequired {
            method: "Spc".to_string(),
        }
        .into();

        assert_eq!(err.code(), "WYRD_DRIFT_400_PROFILE_REQUIRED");
        assert_eq!(err.status(), 400);
        let details = match &err {
            WyrdError::DriftProfileRequired { details, .. } => details,
            other => panic!("wrong variant: {other:?}"),
        };
        assert_eq!(
            details.get("method").and_then(|value| value.as_str()),
            Some("Spc")
        );
    }

    #[test]
    fn catch_all_routes_to_drift_validation_catalog_code() {
        let err: WyrdError = DriftValidationError::OutsideBoundsInverted.into();

        assert_eq!(err.code(), "WYRD_DRIFT_400_VALIDATION");
        assert_eq!(err.status(), 400);
        let details = match &err {
            WyrdError::DriftValidation { details, .. } => details,
            other => panic!("wrong variant: {other:?}"),
        };
        assert_eq!(
            details.get("field").and_then(|value| value.as_str()),
            Some("condition")
        );
        assert_eq!(
            details.get("reason").and_then(|value| value.as_str()),
            Some("lower_gte_upper")
        );
    }
}

#[cfg(test)]
mod envelope_roundtrip_tests {
    use std::collections::BTreeMap;

    use crate::api_version::ApiVersion;
    use crate::card::prompt::PromptSpec;
    use crate::envelope::{Card, CardKind, Metadata, Relationships, Spec};
    use crate::format;
    use crate::ids::CardName;
    use skald_spec::wire::openai_chat::OpenAiMessageContent;
    use skald_spec::{
        OpenAiChatMessage, OpenAiChatRequest, OpenAiChatSettings, Prompt, ProviderRequest,
        ResponseType,
    };
    use wyrd_semver::VersionBlock;

    #[test]
    fn card_yaml_round_trip() {
        let card = Card {
            api_version: ApiVersion::v1(),
            kind: CardKind::Prompt,
            metadata: Metadata {
                name: CardName::new("support_prompt").unwrap(),
                version: Some(VersionBlock::parse("1.0.0").unwrap().into()),
                bump: None,
                space: None,
                uid: None,
                labels: BTreeMap::new(),
                annotations: BTreeMap::new(),
                spec_hash: None,
                artifact_hash: None,
                origin: None,
            },
            spec: Spec::Prompt(prompt_spec()),
            relationships: Relationships::default(),
            status: None,
        };

        let yaml = format::yaml::to_string(&card).unwrap();
        assert!(yaml.contains("apiVersion: wyrd/v1"));
        assert!(!yaml.contains("type: Prompt"));
        assert!(!yaml.contains("api_version:"));
        let decoded = format::yaml::from_str(&yaml).unwrap();
        assert_eq!(decoded, card);
    }

    fn prompt_spec() -> PromptSpec {
        PromptSpec::new(Prompt {
            request: ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
                model: "gpt-4o".to_owned(),
                messages: vec![OpenAiChatMessage {
                    role: "user".to_owned(),
                    content: Some(OpenAiMessageContent::Text("Answer carefully.".to_owned())),
                    name: None,
                    tool_calls: None,
                    tool_call_id: None,
                    refusal: None,
                    annotations: Vec::new(),
                    audio: None,
                }],
                response_format: None,
                stream: None,
                stream_options: None,
                tools: None,
                tool_choice: None,
                parallel_tool_calls: None,
                settings: OpenAiChatSettings::default(),
            }),
            model: "gpt-4o".to_owned(),
            version: None,
            variables: Vec::new(),
            media_variables: Vec::new(),
            response_type: ResponseType::Text,
        })
        .expect("static prompt spec is valid")
    }

    #[test]
    fn phase_1_addendum_fixtures_round_trip() {
        for fixture in [
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/trigger-on-drift.yaml"
            )),
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/operator-remediation.yaml"
            )),
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/service-with-runtime-policy.yaml"
            )),
        ] {
            let decoded: Card = format::yaml::from_str(fixture).unwrap();
            let encoded = format::yaml::to_string(&decoded).unwrap();
            let reparsed: Card = format::yaml::from_str(&encoded).unwrap();
            assert_eq!(reparsed, decoded);
        }
    }

    #[test]
    fn trigger_fixture_uses_native_trigger_kind() {
        let decoded: Card = format::yaml::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/trigger-on-drift.yaml"
        )))
        .unwrap();
        assert_eq!(decoded.kind, CardKind::Trigger);
        assert!(matches!(decoded.spec, Spec::Trigger(_)));
    }

    #[test]
    fn operator_fixture_uses_native_operator_kind() {
        let decoded: Card = format::yaml::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/operator-remediation.yaml"
        )))
        .unwrap();
        assert_eq!(decoded.kind, CardKind::Operator);
        assert!(matches!(decoded.spec, Spec::Operator(_)));
    }
}

#[cfg(test)]
mod eval_card_tests {
    use std::collections::BTreeMap;

    use crate::api_version::ApiVersion;
    use crate::card::eval::EvalSpec;
    use crate::envelope::{Card, CardKind, Metadata, Relationships, Spec};
    use crate::format;
    use crate::ids::CardName;
    use crate::vala::eval::{AssertionTask, ComparisonOperator, EvalTask, JsonPath, TaskId};
    use wyrd_semver::VersionBlock;

    #[test]
    fn card_body_eval_uses_vala_eval_shape() {
        let spec = eval_spec();
        let serialized = serde_json::to_string(&spec).unwrap();
        let back: EvalSpec = serde_json::from_str(&serialized).unwrap();

        assert_eq!(spec, back);
    }

    #[test]
    fn eval_card_envelope_round_trips_with_vala_eval_shape() {
        let card = Card {
            api_version: ApiVersion::v1(),
            kind: CardKind::Eval,
            metadata: Metadata {
                name: CardName::new("quality_eval").unwrap(),
                version: Some(VersionBlock::parse("1.0.0").unwrap().into()),
                bump: None,
                space: None,
                uid: None,
                labels: BTreeMap::new(),
                annotations: BTreeMap::new(),
                spec_hash: None,
                artifact_hash: None,
                origin: None,
            },
            spec: Spec::Eval(eval_spec()),
            relationships: Relationships::default(),
            status: None,
        };

        let yaml = format::yaml::to_string(&card).unwrap();
        assert!(yaml.contains("kind: Eval"));
        assert!(yaml.contains("tasks:"));
        assert!(yaml.contains("kind: assertion"));
        assert!(!yaml.contains("eval_type:"));
        assert!(!yaml.contains("type: Eval"));

        let decoded: Card = format::yaml::from_str(&yaml).unwrap();
        assert_eq!(decoded, card);
    }

    #[test]
    fn legacy_eval_profile_json_no_longer_deserializes() {
        let legacy = r#"{
            "eval_type": "judge",
            "judge_refs": [],
            "assertions": [{"name": "x", "rule": "$.x != null"}]
        }"#;

        let result: Result<EvalSpec, _> = serde_json::from_str(legacy);

        assert!(result.is_err());
    }

    fn eval_spec() -> EvalSpec {
        let mut tasks = BTreeMap::new();
        let id = TaskId::new("a").unwrap();
        tasks.insert(
            id.clone(),
            EvalTask::Assertion(AssertionTask {
                id,
                context_path: Some(JsonPath::new("$.x").unwrap()),
                item_context_path: None,
                operator: ComparisonOperator::IsNotNull,
                expected: serde_json::Value::Null,
                depends_on: vec![],
                condition: None,
            }),
        );
        EvalSpec::new(tasks).expect("static eval spec is valid")
    }
}

#[cfg(test)]
mod kind_tests {
    use crate::envelope::CardKind;
    use proptest::prelude::*;
    use serde_json::json;

    #[test]
    fn native_card_kind_count_is_locked() {
        assert_eq!(CardKind::native().len(), CardKind::NATIVE_COUNT);
        assert_eq!(CardKind::NATIVE_COUNT, 17);
        assert!(
            CardKind::native()
                .iter()
                .any(|kind| matches!(kind, CardKind::Trigger))
        );
        assert!(
            CardKind::native()
                .iter()
                .any(|kind| matches!(kind, CardKind::Operator))
        );
        assert!(
            CardKind::native()
                .iter()
                .any(|kind| matches!(kind, CardKind::Source))
        );
        assert!(
            CardKind::native()
                .iter()
                .any(|kind| matches!(kind, CardKind::External))
        );
    }

    #[test]
    fn native_card_kind_wire_shape_is_pascal_case_string() {
        let json = serde_json::to_value(CardKind::Prompt).unwrap();
        assert_eq!(json, json!("Prompt"));
    }

    #[test]
    fn external_card_kind_wire_shape_is_string() {
        let json = serde_json::to_value(CardKind::External).unwrap();
        assert_eq!(json, json!("External"));
        let decoded: CardKind = serde_json::from_value(json).unwrap();
        assert_eq!(decoded, CardKind::External);
    }

    #[test]
    fn card_kind_schema_is_string_enum_with_registrable_kinds() {
        let schema = schemars::schema_for!(CardKind);
        let value = serde_json::to_value(schema).unwrap();
        let enum_values = value["enum"].as_array().expect("enum schema");
        assert!(enum_values.contains(&json!("Data")));
        assert!(enum_values.contains(&json!("Trigger")));
        assert!(enum_values.contains(&json!("Operator")));
        assert!(enum_values.contains(&json!("Source")));
        assert!(
            !enum_values.contains(&json!("External")),
            "External must not appear in the registrable schema"
        );
        assert!(!enum_values.contains(&json!("Tool")));
        assert!(!enum_values.contains(&json!("Skill")));
        assert!(!enum_values.contains(&json!("SubAgent")));
        assert_eq!(enum_values.len(), 16);
    }

    proptest! {
        #[test]
        fn native_kinds_round_trip(kind in prop::sample::select(CardKind::native().to_vec())) {
            let encoded = serde_json::to_string(&kind).unwrap();
            let decoded: CardKind = serde_json::from_str(&encoded).unwrap();
            prop_assert_eq!(decoded, kind);
        }
    }
}

#[cfg(test)]
mod source_card_tests {
    use std::collections::BTreeMap;

    use crate::card::source::{
        LogConnection, MetricsConnection, ObjectFormat, SourceAuth, SourceKind, SourceSpec,
        SourceValidationError, SqlConnection, TraceConnection,
    };
    use crate::envelope::{Card, CardKind, Spec};
    use crate::error::WyrdError;
    use crate::format;

    // ---------------------------------------------------------------------------
    // Fixture round-trips
    // ---------------------------------------------------------------------------

    #[test]
    fn sql_warehouse_fixture_uses_native_source_kind() {
        let decoded: Card = format::yaml::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/source-sql-warehouse.yaml"
        )))
        .unwrap();
        assert_eq!(decoded.kind, CardKind::Source);
        let Spec::Source(spec) = &decoded.spec else {
            panic!("expected Source spec");
        };
        assert!(matches!(
            spec.source,
            SourceKind::SqlWarehouse {
                connection: SqlConnection::Snowflake { .. }
            }
        ));
        spec.validate().expect("fixture source is valid");
        let encoded = format::yaml::to_string(&decoded).unwrap();
        let reparsed: Card = format::yaml::from_str(&encoded).unwrap();
        assert_eq!(reparsed, decoded);
    }

    #[test]
    fn object_store_fixture_round_trips() {
        let decoded: Card = format::yaml::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/source-object-store-gcs.yaml"
        )))
        .unwrap();
        assert_eq!(decoded.kind, CardKind::Source);
        let Spec::Source(spec) = &decoded.spec else {
            panic!("expected Source spec");
        };
        assert!(matches!(spec.source, SourceKind::ObjectStore { .. }));
        spec.validate().expect("fixture object store is valid");
        let encoded = format::yaml::to_string(&decoded).unwrap();
        let reparsed: Card = format::yaml::from_str(&encoded).unwrap();
        assert_eq!(reparsed, decoded);
    }

    #[test]
    fn metrics_datadog_fixture_round_trips() {
        let decoded: Card = format::yaml::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/source-metrics-datadog.yaml"
        )))
        .unwrap();
        assert_eq!(decoded.kind, CardKind::Source);
        let Spec::Source(spec) = &decoded.spec else {
            panic!("expected Source spec");
        };
        assert!(matches!(
            spec.source,
            SourceKind::Metrics {
                connection: MetricsConnection::Datadog { .. }
            }
        ));
        spec.validate().expect("fixture metrics datadog is valid");
        let encoded = format::yaml::to_string(&decoded).unwrap();
        let reparsed: Card = format::yaml::from_str(&encoded).unwrap();
        assert_eq!(reparsed, decoded);
    }

    #[test]
    fn logs_loki_fixture_round_trips() {
        let decoded: Card = format::yaml::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/source-logs-loki.yaml"
        )))
        .unwrap();
        assert_eq!(decoded.kind, CardKind::Source);
        let Spec::Source(spec) = &decoded.spec else {
            panic!("expected Source spec");
        };
        assert!(matches!(
            spec.source,
            SourceKind::Logs {
                connection: LogConnection::Loki { .. }
            }
        ));
        spec.validate().expect("fixture logs loki is valid");
        let encoded = format::yaml::to_string(&decoded).unwrap();
        let reparsed: Card = format::yaml::from_str(&encoded).unwrap();
        assert_eq!(reparsed, decoded);
    }

    #[test]
    fn traces_tempo_fixture_round_trips() {
        let decoded: Card = format::yaml::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/source-traces-tempo.yaml"
        )))
        .unwrap();
        assert_eq!(decoded.kind, CardKind::Source);
        let Spec::Source(spec) = &decoded.spec else {
            panic!("expected Source spec");
        };
        assert!(matches!(
            spec.source,
            SourceKind::Traces {
                connection: TraceConnection::Tempo { .. }
            }
        ));
        spec.validate().expect("fixture traces tempo is valid");
        let encoded = format::yaml::to_string(&decoded).unwrap();
        let reparsed: Card = format::yaml::from_str(&encoded).unwrap();
        assert_eq!(reparsed, decoded);
    }

    // ---------------------------------------------------------------------------
    // Wire shape assertions
    // ---------------------------------------------------------------------------

    #[test]
    fn object_store_bucket_carries_format() {
        let spec = SourceSpec {
            description: None,
            source: SourceKind::ObjectStore {
                uri: "gs://acme-telemetry/runs".to_owned(),
                format: ObjectFormat::Parquet,
                auth: SourceAuth::None,
            },
            defaults: BTreeMap::new(),
        };
        spec.validate().unwrap();

        let value = serde_json::to_value(&spec).unwrap();
        assert_eq!(value["source"]["kind"], "object_store");
        assert_eq!(value["source"]["format"], "parquet");
    }

    #[test]
    fn metrics_bucket_keeps_vendor_below_kind() {
        let spec = SourceSpec {
            description: None,
            source: SourceKind::Metrics {
                connection: MetricsConnection::Prometheus {
                    endpoint: "https://prom.acme.com/api/v1".to_owned(),
                    auth: SourceAuth::None,
                },
            },
            defaults: BTreeMap::new(),
        };
        let value = serde_json::to_value(&spec).unwrap();
        assert_eq!(value["source"]["kind"], "metrics");
        assert_eq!(value["source"]["connection"]["vendor"], "prometheus");
    }

    // ---------------------------------------------------------------------------
    // Validation — coordinates
    // ---------------------------------------------------------------------------

    #[test]
    fn empty_coordinate_is_rejected() {
        let spec = SourceSpec {
            description: None,
            source: SourceKind::SqlWarehouse {
                connection: SqlConnection::BigQuery {
                    project: "  ".to_owned(),
                    dataset: None,
                    location: None,
                    auth: SourceAuth::None,
                },
            },
            defaults: BTreeMap::new(),
        };
        assert_eq!(
            spec.validate(),
            Err(SourceValidationError::EmptyField { field: "project" })
        );
    }

    #[test]
    fn object_store_empty_uri_is_rejected() {
        let spec = SourceSpec {
            description: None,
            source: SourceKind::ObjectStore {
                uri: "  ".to_owned(),
                format: ObjectFormat::Parquet,
                auth: SourceAuth::None,
            },
            defaults: BTreeMap::new(),
        };
        assert_eq!(
            spec.validate(),
            Err(SourceValidationError::EmptyField { field: "uri" })
        );
    }

    #[test]
    fn object_store_uri_with_embedded_credentials_is_rejected() {
        let spec = SourceSpec {
            description: None,
            source: SourceKind::ObjectStore {
                uri: "s3://AKIAIOSFODNN7EXAMPLE:wJalrXUtnFEMI@my-bucket/prefix".to_owned(),
                format: ObjectFormat::Parquet,
                auth: SourceAuth::None,
            },
            defaults: BTreeMap::new(),
        };
        assert_eq!(
            spec.validate(),
            Err(SourceValidationError::EmbeddedCredential { field: "uri" })
        );
    }

    #[test]
    fn object_store_uri_with_at_sign_but_no_password_is_accepted() {
        let spec = SourceSpec {
            description: None,
            source: SourceKind::ObjectStore {
                uri: "s3://user@my-bucket/prefix".to_owned(),
                format: ObjectFormat::Parquet,
                auth: SourceAuth::None,
            },
            defaults: BTreeMap::new(),
        };
        assert!(spec.validate().is_ok());
    }

    // ---------------------------------------------------------------------------
    // Validation — auth
    // ---------------------------------------------------------------------------

    #[test]
    fn empty_auth_env_name_is_rejected() {
        let spec = SourceSpec {
            description: None,
            source: SourceKind::Metrics {
                connection: MetricsConnection::Datadog {
                    site: "datadoghq.com".to_owned(),
                    api_scopes: vec!["metrics_read".to_owned()],
                    auth: SourceAuth::Env { env: String::new() },
                },
            },
            defaults: BTreeMap::new(),
        };
        assert_eq!(
            spec.validate(),
            Err(SourceValidationError::EmptyField { field: "auth.env" })
        );
    }

    #[test]
    fn auth_basic_round_trips() {
        let spec = SourceSpec {
            description: None,
            source: SourceKind::SqlWarehouse {
                connection: SqlConnection::Postgres {
                    host: "pg.acme.com".to_owned(),
                    port: Some(5432),
                    database: "telemetry".to_owned(),
                    sslmode: Some("require".to_owned()),
                    auth: SourceAuth::Basic {
                        username: "wyrd_reader".to_owned(),
                        password_env: "PG_PASSWORD".to_owned(),
                    },
                },
            },
            defaults: BTreeMap::new(),
        };
        spec.validate().unwrap();
        let json = serde_json::to_value(&spec).unwrap();
        assert_eq!(json["source"]["connection"]["auth"]["scheme"], "basic");
        assert_eq!(
            json["source"]["connection"]["auth"]["username"],
            "wyrd_reader"
        );
        assert_eq!(
            json["source"]["connection"]["auth"]["password_env"],
            "PG_PASSWORD"
        );
    }

    #[test]
    fn auth_basic_rejects_empty_username() {
        let auth = SourceSpec {
            description: None,
            source: SourceKind::SqlWarehouse {
                connection: SqlConnection::Postgres {
                    host: "pg.acme.com".to_owned(),
                    port: None,
                    database: "telemetry".to_owned(),
                    sslmode: None,
                    auth: SourceAuth::Basic {
                        username: "  ".to_owned(),
                        password_env: "PG_PASSWORD".to_owned(),
                    },
                },
            },
            defaults: BTreeMap::new(),
        };
        assert_eq!(
            auth.validate(),
            Err(SourceValidationError::EmptyField {
                field: "auth.username"
            })
        );
    }

    #[test]
    fn auth_basic_rejects_empty_password_env() {
        let spec = SourceSpec {
            description: None,
            source: SourceKind::SqlWarehouse {
                connection: SqlConnection::Postgres {
                    host: "pg.acme.com".to_owned(),
                    port: None,
                    database: "telemetry".to_owned(),
                    sslmode: None,
                    auth: SourceAuth::Basic {
                        username: "wyrd_reader".to_owned(),
                        password_env: "".to_owned(),
                    },
                },
            },
            defaults: BTreeMap::new(),
        };
        assert_eq!(
            spec.validate(),
            Err(SourceValidationError::EmptyField {
                field: "auth.password_env"
            })
        );
    }

    #[test]
    fn auth_basic_debug_redacts_username() {
        let auth = SourceAuth::Basic {
            username: "john@example.com".to_owned(),
            password_env: "PG_PASSWORD".to_owned(),
        };
        let debug = format!("{auth:?}");
        assert!(
            !debug.contains("john@example.com"),
            "username must be redacted in Debug output"
        );
        assert!(debug.contains("[redacted]"));
        assert!(debug.contains("PG_PASSWORD"));
    }

    #[test]
    fn auth_multi_env_round_trips() {
        let mut vars = BTreeMap::new();
        vars.insert("api_key".to_owned(), "DD_API_KEY".to_owned());
        vars.insert("app_key".to_owned(), "DD_APP_KEY".to_owned());
        let spec = SourceSpec {
            description: None,
            source: SourceKind::Metrics {
                connection: MetricsConnection::Datadog {
                    site: "datadoghq.com".to_owned(),
                    api_scopes: vec!["metrics_read".to_owned()],
                    auth: SourceAuth::MultiEnv { vars },
                },
            },
            defaults: BTreeMap::new(),
        };
        spec.validate().unwrap();
        let json = serde_json::to_value(&spec).unwrap();
        assert_eq!(json["source"]["connection"]["auth"]["scheme"], "multi_env");
        assert_eq!(
            json["source"]["connection"]["auth"]["vars"]["api_key"],
            "DD_API_KEY"
        );
    }

    #[test]
    fn auth_multi_env_rejects_empty_map() {
        let spec = SourceSpec {
            description: None,
            source: SourceKind::Metrics {
                connection: MetricsConnection::Datadog {
                    site: "datadoghq.com".to_owned(),
                    api_scopes: vec![],
                    auth: SourceAuth::MultiEnv {
                        vars: BTreeMap::new(),
                    },
                },
            },
            defaults: BTreeMap::new(),
        };
        assert_eq!(
            spec.validate(),
            Err(SourceValidationError::EmptyField { field: "auth.vars" })
        );
    }

    #[test]
    fn auth_multi_env_rejects_whitespace_only_key() {
        let mut vars = BTreeMap::new();
        vars.insert("  ".to_owned(), "DD_API_KEY".to_owned());
        let spec = SourceSpec {
            description: None,
            source: SourceKind::Metrics {
                connection: MetricsConnection::Datadog {
                    site: "datadoghq.com".to_owned(),
                    api_scopes: vec![],
                    auth: SourceAuth::MultiEnv { vars },
                },
            },
            defaults: BTreeMap::new(),
        };
        assert_eq!(
            spec.validate(),
            Err(SourceValidationError::EmptyField {
                field: "auth.vars.key"
            })
        );
    }

    #[test]
    fn auth_multi_env_rejects_empty_value() {
        let mut vars = BTreeMap::new();
        vars.insert("api_key".to_owned(), "  ".to_owned());
        let spec = SourceSpec {
            description: None,
            source: SourceKind::Metrics {
                connection: MetricsConnection::Datadog {
                    site: "datadoghq.com".to_owned(),
                    api_scopes: vec![],
                    auth: SourceAuth::MultiEnv { vars },
                },
            },
            defaults: BTreeMap::new(),
        };
        assert_eq!(
            spec.validate(),
            Err(SourceValidationError::EmptyField {
                field: "auth.vars.value"
            })
        );
    }

    #[test]
    fn auth_secret_store_round_trips() {
        let spec = SourceSpec {
            description: None,
            source: SourceKind::SqlWarehouse {
                connection: SqlConnection::Snowflake {
                    account: "acme-prod".to_owned(),
                    warehouse: "analytics".to_owned(),
                    database: "telemetry".to_owned(),
                    schema: None,
                    role: None,
                    auth: SourceAuth::SecretStore {
                        provider: "vault".to_owned(),
                        name: "secret/data/snowflake/reader".to_owned(),
                    },
                },
            },
            defaults: BTreeMap::new(),
        };
        spec.validate().unwrap();
        let json = serde_json::to_value(&spec).unwrap();
        assert_eq!(
            json["source"]["connection"]["auth"]["scheme"],
            "secret_store"
        );
        assert_eq!(json["source"]["connection"]["auth"]["provider"], "vault");
        assert_eq!(
            json["source"]["connection"]["auth"]["name"],
            "secret/data/snowflake/reader"
        );
    }

    #[test]
    fn auth_secret_store_rejects_empty_provider() {
        let spec = SourceSpec {
            description: None,
            source: SourceKind::SqlWarehouse {
                connection: SqlConnection::Snowflake {
                    account: "acme-prod".to_owned(),
                    warehouse: "analytics".to_owned(),
                    database: "telemetry".to_owned(),
                    schema: None,
                    role: None,
                    auth: SourceAuth::SecretStore {
                        provider: "".to_owned(),
                        name: "secret/snowflake".to_owned(),
                    },
                },
            },
            defaults: BTreeMap::new(),
        };
        assert_eq!(
            spec.validate(),
            Err(SourceValidationError::EmptyField {
                field: "auth.provider"
            })
        );
    }

    #[test]
    fn auth_secret_store_rejects_empty_name() {
        let spec = SourceSpec {
            description: None,
            source: SourceKind::SqlWarehouse {
                connection: SqlConnection::Snowflake {
                    account: "acme-prod".to_owned(),
                    warehouse: "analytics".to_owned(),
                    database: "telemetry".to_owned(),
                    schema: None,
                    role: None,
                    auth: SourceAuth::SecretStore {
                        provider: "vault".to_owned(),
                        name: "  ".to_owned(),
                    },
                },
            },
            defaults: BTreeMap::new(),
        };
        assert_eq!(
            spec.validate(),
            Err(SourceValidationError::EmptyField { field: "auth.name" })
        );
    }

    // ---------------------------------------------------------------------------
    // Validation — per vendor (Logs, Traces, SQL/Postgres, Metrics/Cloudwatch)
    // ---------------------------------------------------------------------------

    #[test]
    fn sql_postgres_round_trips() {
        let spec = SourceSpec {
            description: None,
            source: SourceKind::SqlWarehouse {
                connection: SqlConnection::Postgres {
                    host: "pg.acme.com".to_owned(),
                    port: Some(5432),
                    database: "telemetry".to_owned(),
                    sslmode: Some("verify-full".to_owned()),
                    auth: SourceAuth::Env {
                        env: "PG_URI".to_owned(),
                    },
                },
            },
            defaults: BTreeMap::new(),
        };
        spec.validate().unwrap();
        let json = serde_json::to_value(&spec).unwrap();
        assert_eq!(json["source"]["connection"]["vendor"], "postgres");
        assert_eq!(json["source"]["connection"]["host"], "pg.acme.com");
    }

    #[test]
    fn sql_postgres_empty_host_is_rejected() {
        let spec = SourceSpec {
            description: None,
            source: SourceKind::SqlWarehouse {
                connection: SqlConnection::Postgres {
                    host: "".to_owned(),
                    port: None,
                    database: "telemetry".to_owned(),
                    sslmode: None,
                    auth: SourceAuth::None,
                },
            },
            defaults: BTreeMap::new(),
        };
        assert_eq!(
            spec.validate(),
            Err(SourceValidationError::EmptyField { field: "host" })
        );
    }

    #[test]
    fn metrics_cloudwatch_round_trips() {
        let spec = SourceSpec {
            description: None,
            source: SourceKind::Metrics {
                connection: MetricsConnection::Cloudwatch {
                    region: "us-east-1".to_owned(),
                    namespace: Some("AWS/SageMaker".to_owned()),
                    auth: SourceAuth::Env {
                        env: "AWS_WEB_IDENTITY_TOKEN_FILE".to_owned(),
                    },
                },
            },
            defaults: BTreeMap::new(),
        };
        spec.validate().unwrap();
        let json = serde_json::to_value(&spec).unwrap();
        assert_eq!(json["source"]["connection"]["vendor"], "cloudwatch");
        assert_eq!(json["source"]["connection"]["region"], "us-east-1");
    }

    #[test]
    fn metrics_cloudwatch_empty_region_is_rejected() {
        let spec = SourceSpec {
            description: None,
            source: SourceKind::Metrics {
                connection: MetricsConnection::Cloudwatch {
                    region: "  ".to_owned(),
                    namespace: None,
                    auth: SourceAuth::None,
                },
            },
            defaults: BTreeMap::new(),
        };
        assert_eq!(
            spec.validate(),
            Err(SourceValidationError::EmptyField { field: "region" })
        );
    }

    #[test]
    fn logs_elasticsearch_round_trips() {
        let spec = SourceSpec {
            description: None,
            source: SourceKind::Logs {
                connection: LogConnection::Elasticsearch {
                    endpoint: "https://es.acme.com".to_owned(),
                    index: Some("prod-logs-*".to_owned()),
                    auth: SourceAuth::Basic {
                        username: "elastic".to_owned(),
                        password_env: "ES_PASSWORD".to_owned(),
                    },
                },
            },
            defaults: BTreeMap::new(),
        };
        spec.validate().unwrap();
        let json = serde_json::to_value(&spec).unwrap();
        assert_eq!(json["source"]["connection"]["vendor"], "elasticsearch");
    }

    #[test]
    fn logs_splunk_round_trips() {
        let spec = SourceSpec {
            description: None,
            source: SourceKind::Logs {
                connection: LogConnection::Splunk {
                    endpoint: "https://splunk.acme.com:8089".to_owned(),
                    auth: SourceAuth::Env {
                        env: "SPLUNK_TOKEN".to_owned(),
                    },
                },
            },
            defaults: BTreeMap::new(),
        };
        spec.validate().unwrap();
        let json = serde_json::to_value(&spec).unwrap();
        assert_eq!(json["source"]["connection"]["vendor"], "splunk");
    }

    #[test]
    fn logs_empty_endpoint_is_rejected() {
        let spec = SourceSpec {
            description: None,
            source: SourceKind::Logs {
                connection: LogConnection::Loki {
                    endpoint: "".to_owned(),
                    auth: SourceAuth::None,
                },
            },
            defaults: BTreeMap::new(),
        };
        assert_eq!(
            spec.validate(),
            Err(SourceValidationError::EmptyField { field: "endpoint" })
        );
    }

    #[test]
    fn traces_datadog_apm_round_trips() {
        let spec = SourceSpec {
            description: None,
            source: SourceKind::Traces {
                connection: TraceConnection::DatadogApm {
                    site: "datadoghq.com".to_owned(),
                    auth: SourceAuth::MultiEnv {
                        vars: {
                            let mut m = BTreeMap::new();
                            m.insert("api_key".to_owned(), "DD_API_KEY".to_owned());
                            m.insert("app_key".to_owned(), "DD_APP_KEY".to_owned());
                            m
                        },
                    },
                },
            },
            defaults: BTreeMap::new(),
        };
        spec.validate().unwrap();
        let json = serde_json::to_value(&spec).unwrap();
        assert_eq!(json["source"]["connection"]["vendor"], "datadog_apm");
        assert_eq!(json["source"]["connection"]["site"], "datadoghq.com");
    }

    #[test]
    fn traces_jaeger_round_trips() {
        let spec = SourceSpec {
            description: None,
            source: SourceKind::Traces {
                connection: TraceConnection::Jaeger {
                    endpoint: "https://jaeger.acme.com".to_owned(),
                    auth: SourceAuth::None,
                },
            },
            defaults: BTreeMap::new(),
        };
        spec.validate().unwrap();
        let json = serde_json::to_value(&spec).unwrap();
        assert_eq!(json["source"]["connection"]["vendor"], "jaeger");
    }

    #[test]
    fn traces_empty_endpoint_is_rejected() {
        let spec = SourceSpec {
            description: None,
            source: SourceKind::Traces {
                connection: TraceConnection::Tempo {
                    endpoint: "  ".to_owned(),
                    auth: SourceAuth::None,
                },
            },
            defaults: BTreeMap::new(),
        };
        assert_eq!(
            spec.validate(),
            Err(SourceValidationError::EmptyField { field: "endpoint" })
        );
    }

    // ---------------------------------------------------------------------------
    // WyrdError wire format
    // ---------------------------------------------------------------------------

    #[test]
    fn source_validation_error_maps_to_wyrd_error_code_and_details() {
        let error: WyrdError = SourceValidationError::EmptyField { field: "endpoint" }.into();
        assert_eq!(error.code(), "WYRD_SOURCE_400_VALIDATION");
        assert_eq!(error.as_problem_json()["details"]["field"], "endpoint");

        let error: WyrdError = SourceValidationError::EmbeddedCredential { field: "uri" }.into();
        assert_eq!(error.code(), "WYRD_SOURCE_400_VALIDATION");
        assert_eq!(error.as_problem_json()["details"]["field"], "uri");
    }

    // ---------------------------------------------------------------------------
    // URI credential check edge cases
    // ---------------------------------------------------------------------------

    #[test]
    fn object_store_uri_with_version_tag_in_path_is_accepted() {
        let spec = SourceSpec {
            description: None,
            source: SourceKind::ObjectStore {
                uri: "gs://bucket/checkpoints:v1@run-abc".to_owned(),
                format: ObjectFormat::Parquet,
                auth: SourceAuth::None,
            },
            defaults: BTreeMap::new(),
        };
        assert!(spec.validate().is_ok());
    }

    #[test]
    fn object_store_uri_with_percent_encoded_credentials_is_rejected() {
        let spec = SourceSpec {
            description: None,
            source: SourceKind::ObjectStore {
                uri: "gs://user%3Apass%40host/bucket".to_owned(),
                format: ObjectFormat::Parquet,
                auth: SourceAuth::None,
            },
            defaults: BTreeMap::new(),
        };
        assert_eq!(
            spec.validate(),
            Err(SourceValidationError::EmbeddedCredential { field: "uri" })
        );
    }
}
