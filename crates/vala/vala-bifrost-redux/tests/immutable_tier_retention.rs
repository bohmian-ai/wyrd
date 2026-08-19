use std::sync::Arc;

use arrow::array::{Int64Array, RecordBatch};
use arrow::datatypes::{DataType, Field, Schema};
use chrono::NaiveDate;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::file_list_writer::FileListCommitKey;
use vala_bifrost_redux::scribe::memtable::Memtable;
use vala_bifrost_redux::scribe::seal_key::{EventDay, SealKey};
use vala_bifrost_redux::scribe::wal::{ScribeAppendMeta, WalLsn};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

fn event() -> AuditEvent {
    AuditEvent {
        request_id: wyrd_spec::request_id::RequestId::now_v7(),
        trace_id: None,
        operation: "immutable.test".to_owned(),
        resource: "vala.bifrost.events".to_owned(),
        card_ref: None,
        principal_id: wyrd_spec::auth::PrincipalId::new(uuid::Uuid::now_v7()),
        principal_kind: wyrd_spec::auth::PrincipalKindTag::Service,
        auth_method: AuthMethod::Jwt,
        permission: "test".to_owned(),
        decision: AuditDecision::Allow,
        result: AuditResult::Success,
        payload_summary: "test".to_owned(),
        detail: None,
    }
}

#[test]
fn pending_is_readable_and_only_committed_generation_retires() {
    let tenant = DataTenantId::new_v7();
    let table = TableRef::new(BifrostNamespace::Bifrost, "events");
    let key = SealKey::new(
        tenant,
        table.clone(),
        EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 14).expect("date")),
    );
    let schema = Arc::new(Schema::new(vec![Field::new(
        "value",
        DataType::Int64,
        false,
    )]));
    let batch =
        RecordBatch::try_new(schema, vec![Arc::new(Int64Array::from(vec![1]))]).expect("batch");
    let memtable = Memtable::new();
    memtable
        .insert(
            &key,
            event(),
            ScribeAppendMeta {
                batch_id: [1; 16],
                schema_fingerprint: [0; 32],
                data_digest: [0; 32],
                data_len: 0,
                payload_digest: [0; 32],
                payload_len: 0,
                slice_index: 0,
                slice_count: 1,
                rows_accepted: 1,
                wal_lsn_min: WalLsn::new(11),
                wal_lsn_max: WalLsn::new(11),
                seal_key: key.as_path_components(),
            },
            batch,
        )
        .expect("insert");
    let frozen = memtable.freeze(&key).expect("freeze");
    assert_eq!(memtable.pending_generation_count().expect("pending"), 1);
    // A pending generation must not retire, even on the first sweep.
    assert!(
        memtable.sweep_once().expect("pending sweep").is_empty(),
        "PendingCommit generation must not retire before SQL commit"
    );

    memtable
        .complete_post_commit(
            frozen.seal_id,
            FileListCommitKey {
                data_tenant_id: tenant,
                namespace: "vala.bifrost".to_owned(),
                table_name: "events".to_owned(),
                node_id: uuid::Uuid::nil(),
                writer_epoch: 1,
                wal_lsn_min: 11,
                wal_lsn_max: 11,
            },
        )
        .expect("complete");
    // After commit, the generation is immediately retirement-eligible.
    assert_eq!(
        memtable.sweep_once().expect("committed sweep").len(),
        1,
        "committed generation retires at the first sweep"
    );
}
