use std::collections::HashMap;

use wyrd_spec::card::data::{
    ColValue, DataInterface, DataSchema, DataSpec, DataSplit, DataStats, PandasMeta,
    ParquetCompression, SplitStrategy,
};
use wyrd_spec::card::{FieldSpec, Inequality};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, ColumnName, SplitName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::version::VersionBlock;

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
        space: None,
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
    let spec = DataSpec::new(
        interface(),
        schema(),
        vec![artifact.clone()],
        HashMap::from([(split("train"), train_split)]),
        vec![col("target")],
        None,
        stats(),
    )
    .unwrap();

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
