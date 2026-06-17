use std::collections::{BTreeMap, HashMap};

use wyrd_spec::card::data::{
    ArrowFormat, ArrowMeta, ColValue, ColorMode, CustomDataMeta, DataInterface, DataSchema,
    DataSpec, DataSplit, DataStats, HuggingfaceMeta, ImageFormat, ImageMeta, JsonlCompression,
    JsonlMeta, NumpyFormat, NumpyMeta, PandasMeta, ParquetCompression, ParquetMeta, PolarsMeta,
    SplitStrategy, SqlLogic, SqlMeta, TextMeta, TorchMeta, TorchSaveFormat,
};
use wyrd_spec::card::{FieldSpec, Inequality};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, ColumnName, QueryName, SpaceName, SplitName};
use wyrd_spec::reference::CardRef;
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
    DataSpec::new(
        interface,
        schema,
        Vec::new(),
        HashMap::new(),
        Vec::new(),
        sql,
        stats(),
    )
    .unwrap()
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
