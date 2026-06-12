use std::collections::BTreeMap;

use wyrd_spec::card::source::{
    MetricsConnection, ObjectFormat, SourceAuth, SourceKind, SourceSpec, SourceValidationError,
    SqlConnection,
};
use wyrd_spec::envelope::{Card, CardKind, Spec};
use wyrd_spec::format;

#[test]
fn sql_warehouse_fixture_uses_native_source_kind() {
    let decoded: Card =
        format::yaml::from_str(include_str!("../fixtures/source-sql-warehouse.yaml")).unwrap();
    assert_eq!(decoded.kind, CardKind::Source);
    let Spec::Source(spec) = &decoded.spec else {
        panic!("expected Source spec");
    };
    assert!(matches!(
        spec.kind,
        SourceKind::SqlWarehouse {
            connection: SqlConnection::Snowflake { .. }
        }
    ));
    spec.validate().expect("fixture source is valid");
}

#[test]
fn source_card_yaml_round_trips() {
    let decoded: Card =
        format::yaml::from_str(include_str!("../fixtures/source-sql-warehouse.yaml")).unwrap();
    let encoded = format::yaml::to_string(&decoded).unwrap();
    let reparsed: Card = format::yaml::from_str(&encoded).unwrap();
    assert_eq!(reparsed, decoded);
}

#[test]
fn object_store_bucket_carries_format() {
    let spec = SourceSpec {
        description: None,
        kind: SourceKind::ObjectStore {
            uri: "gs://acme-telemetry/runs".to_owned(),
            format: ObjectFormat::Parquet,
        },
        defaults: BTreeMap::new(),
    };
    spec.validate().unwrap();

    let value = serde_json::to_value(&spec).unwrap();
    assert_eq!(value["kind"]["kind"], "object_store");
    assert_eq!(value["kind"]["format"], "parquet");
}

#[test]
fn metrics_bucket_keeps_vendor_below_kind() {
    let spec = SourceSpec {
        description: None,
        kind: SourceKind::Metrics {
            connection: MetricsConnection::Prometheus {
                endpoint: "https://prom.acme.com/api/v1".to_owned(),
                auth: SourceAuth::None,
            },
        },
        defaults: BTreeMap::new(),
    };
    let value = serde_json::to_value(&spec).unwrap();
    // The bucket is the top-level discriminator; the vendor is nested.
    assert_eq!(value["kind"]["kind"], "metrics");
    assert_eq!(value["kind"]["connection"]["vendor"], "prometheus");
}

#[test]
fn empty_coordinate_is_rejected() {
    let spec = SourceSpec {
        description: None,
        kind: SourceKind::SqlWarehouse {
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
fn empty_auth_env_name_is_rejected() {
    let auth_spec = SourceSpec {
        description: None,
        kind: SourceKind::Metrics {
            connection: MetricsConnection::Datadog {
                site: "datadoghq.com".to_owned(),
                scopes: vec!["metrics_read".to_owned()],
                auth: SourceAuth::Env { env: String::new() },
            },
        },
        defaults: BTreeMap::new(),
    };
    assert_eq!(
        auth_spec.validate(),
        Err(SourceValidationError::EmptyField { field: "auth.env" })
    );
}
