use std::collections::BTreeMap;

use wyrd_spec::card::source::{
    LogConnection, MetricsConnection, ObjectFormat, SourceAuth, SourceKind, SourceSpec,
    SourceValidationError, SqlConnection, TraceConnection,
};
use wyrd_spec::envelope::{Card, CardKind, Spec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::format;

// ---------------------------------------------------------------------------
// Fixture round-trips
// ---------------------------------------------------------------------------

#[test]
fn sql_warehouse_fixture_uses_native_source_kind() {
    let decoded: Card =
        format::yaml::from_str(include_str!("../fixtures/source-sql-warehouse.yaml")).unwrap();
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
    let decoded: Card =
        format::yaml::from_str(include_str!("../fixtures/source-object-store-gcs.yaml")).unwrap();
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
    let decoded: Card =
        format::yaml::from_str(include_str!("../fixtures/source-metrics-datadog.yaml")).unwrap();
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
    let decoded: Card =
        format::yaml::from_str(include_str!("../fixtures/source-logs-loki.yaml")).unwrap();
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
    let decoded: Card =
        format::yaml::from_str(include_str!("../fixtures/source-traces-tempo.yaml")).unwrap();
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
    assert_eq!(json["source"]["connection"]["auth"]["username"], "wyrd_reader");
    assert_eq!(json["source"]["connection"]["auth"]["password_env"], "PG_PASSWORD");
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
        Err(SourceValidationError::EmptyField { field: "auth.username" })
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
        Err(SourceValidationError::EmptyField { field: "auth.password_env" })
    );
}

#[test]
fn auth_basic_debug_redacts_username() {
    let auth = SourceAuth::Basic {
        username: "john@example.com".to_owned(),
        password_env: "PG_PASSWORD".to_owned(),
    };
    let debug = format!("{auth:?}");
    assert!(!debug.contains("john@example.com"), "username must be redacted in Debug output");
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
    assert_eq!(json["source"]["connection"]["auth"]["vars"]["api_key"], "DD_API_KEY");
}

#[test]
fn auth_multi_env_rejects_empty_map() {
    let spec = SourceSpec {
        description: None,
        source: SourceKind::Metrics {
            connection: MetricsConnection::Datadog {
                site: "datadoghq.com".to_owned(),
                api_scopes: vec![],
                auth: SourceAuth::MultiEnv { vars: BTreeMap::new() },
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
        Err(SourceValidationError::EmptyField { field: "auth.vars.key" })
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
        Err(SourceValidationError::EmptyField { field: "auth.vars.value" })
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
    assert_eq!(json["source"]["connection"]["auth"]["scheme"], "secret_store");
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
        Err(SourceValidationError::EmptyField { field: "auth.provider" })
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
                auth: SourceAuth::Env { env: "PG_URI".to_owned() },
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
                auth: SourceAuth::Env { env: "AWS_WEB_IDENTITY_TOKEN_FILE".to_owned() },
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
                auth: SourceAuth::Env { env: "SPLUNK_TOKEN".to_owned() },
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
