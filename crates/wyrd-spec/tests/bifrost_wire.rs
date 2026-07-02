//! Contract tests for the Arrow-free Bifrost wire types in `wyrd_spec::vala::api`.

use schemars::schema_for;
use wyrd_spec::vala::api::{
    AsyncJobState, AsyncQueryResponse, AsyncQueryStatus, BifrostTableDescription, BifrostTableEntry,
    DataTypeSpec, ExecutorAvailability, FieldSpec, JobUid, PartitionColumnSpec,
    PartitionTransformWire, QueryParam, RegisterOutcome, RegisterTableRequest, RegisterTableResponse,
    SyncQueryRequest, TableScopeWire, TableStatus, TimeUnit,
};

fn bifrost_wire_round_trip<T>(value: &T) -> T
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let json = serde_json::to_string(value).expect("serialize");
    let back: T = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(*value, back, "round-trip mismatch");
    back
}

#[test]
fn bifrost_wire_field_spec_round_trips_and_defaults_nullable_true() {
    let spec = FieldSpec {
        name: "value".to_string(),
        data_type: DataTypeSpec::Int64,
        nullable: false,
        metadata: Default::default(),
    };
    bifrost_wire_round_trip(&spec);

    // nullable defaults to true when absent; empty metadata is omitted on the wire.
    let json = serde_json::to_value(&spec).expect("serialize");
    assert!(json.get("metadata").is_none(), "empty metadata must be skipped");

    let minimal: FieldSpec =
        serde_json::from_str(r#"{"name":"x","data_type":"Utf8"}"#).expect("deserialize minimal");
    assert!(minimal.nullable, "nullable must default to true");
    assert!(minimal.metadata.is_empty());
}

#[test]
fn bifrost_wire_field_spec_carries_correlation_metadata() {
    let mut spec = FieldSpec {
        name: "card_ref".to_string(),
        data_type: DataTypeSpec::Utf8,
        nullable: true,
        metadata: Default::default(),
    };
    spec.metadata
        .insert("wyrd:column_class".to_string(), "correlation".to_string());
    let back = bifrost_wire_round_trip(&spec);
    assert_eq!(
        back.metadata.get("wyrd:column_class").map(String::as_str),
        Some("correlation")
    );
}

#[test]
fn bifrost_wire_table_scope_wire_defaults_to_tenant_owned() {
    assert_eq!(TableScopeWire::default(), TableScopeWire::TenantOwned);
}

#[test]
fn bifrost_wire_register_request_defaults_partition_and_scope() {
    let req: RegisterTableRequest = serde_json::from_str(
        r#"{"namespace":"vala.bifrost","name":"events","fields":[]}"#,
    )
    .expect("deserialize");
    assert!(req.partition_columns.is_empty());
    assert_eq!(req.scope, TableScopeWire::TenantOwned);
}

#[test]
fn bifrost_wire_nested_and_recursive_data_types_round_trip() {
    let spec = FieldSpec {
        name: "nested".to_string(),
        data_type: DataTypeSpec::Struct(vec![
            FieldSpec {
                name: "tags".to_string(),
                data_type: DataTypeSpec::List(Box::new(DataTypeSpec::Utf8)),
                nullable: true,
                metadata: Default::default(),
            },
            FieldSpec {
                name: "ts".to_string(),
                data_type: DataTypeSpec::Timestamp {
                    unit: TimeUnit::Microsecond,
                    tz: Some("UTC".to_string()),
                },
                nullable: false,
                metadata: Default::default(),
            },
        ]),
        nullable: true,
        metadata: Default::default(),
    };
    bifrost_wire_round_trip(&spec);
}

#[test]
fn bifrost_wire_table_entry_and_description_round_trip() {
    let entry = BifrostTableEntry {
        namespace: "vala.bifrost".to_string(),
        name: "events".to_string(),
        table_uid: "ab".repeat(16),
        scope: TableScopeWire::SystemShared,
        status: TableStatus::Active,
        fingerprint: "01".repeat(32),
        partition_columns: vec!["day".to_string()],
        registered_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let desc = BifrostTableDescription {
        entry: entry.clone(),
        fields: vec![FieldSpec {
            name: "value".to_string(),
            data_type: DataTypeSpec::Int64,
            nullable: false,
            metadata: Default::default(),
        }],
    };
    bifrost_wire_round_trip(&entry);
    bifrost_wire_round_trip(&desc);
}

#[test]
fn bifrost_wire_query_types_round_trip() {
    bifrost_wire_round_trip(&SyncQueryRequest {
        sql: "SELECT 1".to_string(),
        params: vec![
            QueryParam::Null,
            QueryParam::Bool(true),
            QueryParam::Int(7),
            QueryParam::Float(1.5),
            QueryParam::Text("x".to_string()),
        ],
    });
    let job = JobUid(uuid::Uuid::now_v7());
    bifrost_wire_round_trip(&AsyncQueryResponse {
        job_uid: job,
        state: AsyncJobState::Queued,
        executor_availability: ExecutorAvailability::PendingStage5,
    });
    bifrost_wire_round_trip(&AsyncQueryStatus {
        job_uid: job,
        state: AsyncJobState::Failed,
        executor_availability: ExecutorAvailability::PendingStage5,
        error_code: Some("WYRD_VALA_400_QUERY_INVALID_SQL".to_string()),
        error_detail: Some("not a SELECT".to_string()),
    });
}

#[test]
fn bifrost_wire_register_response_and_partition_round_trip() {
    bifrost_wire_round_trip(&RegisterTableResponse {
        outcome: RegisterOutcome::Created,
        table_uid: "ab".repeat(16),
        fingerprint: "01".repeat(32),
    });
    bifrost_wire_round_trip(&PartitionColumnSpec {
        column: "day".to_string(),
        transform: PartitionTransformWire::Bucket { n: 16 },
    });
}

#[test]
fn bifrost_wire_schema_for_wire_types_does_not_panic() {
    let _ = schema_for!(BifrostTableEntry);
    let _ = schema_for!(BifrostTableDescription);
    let _ = schema_for!(DataTypeSpec);
    let _ = schema_for!(FieldSpec);
    let _ = schema_for!(RegisterTableRequest);
    let _ = schema_for!(RegisterTableResponse);
    let _ = schema_for!(SyncQueryRequest);
    let _ = schema_for!(AsyncQueryResponse);
    let _ = schema_for!(AsyncQueryStatus);
    let _ = schema_for!(QueryParam);
    let _ = schema_for!(JobUid);
}
