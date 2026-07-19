use chrono::NaiveDate;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::file_list_writer::FileListCommitKey;
use vala_bifrost_redux::scribe::seal::{PostCommitBatch, PostCommitToken};
use vala_bifrost_redux::scribe::seal_key::{EventDay, SealKey};
use vala_bifrost_redux::scribe::wal::WalLsn;
use wyrd_spec::DataTenantId;

#[test]
fn force_seal_result_is_only_a_post_commit_capability_batch() {
    let tenant = DataTenantId::new_v7();
    let table = TableRef::new(BifrostNamespace::Bifrost, "events");
    let key = SealKey::new(
        tenant,
        table,
        EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 14).expect("date")),
    );
    let token = PostCommitToken {
        seal_id: 9,
        seal_key: key,
        file_list_key: FileListCommitKey {
            data_tenant_id: tenant,
            namespace: "vala.bifrost".to_owned(),
            table_name: "events".to_owned(),
            node_id: uuid::Uuid::nil(),
            writer_epoch: 1,
            wal_lsn_min: 11,
            wal_lsn_max: 11,
        },
        file_list_row_id: uuid::Uuid::nil(),
        wal_lsn_min: WalLsn::new(11),
        wal_lsn_max: WalLsn::new(11),
    };
    let batch: PostCommitBatch = token.clone().into();
    assert_eq!(batch.0.len(), 1);
    assert_eq!(batch.0[0].seal_id, token.seal_id);
    assert_eq!(batch.0[0].wal_lsn_max, WalLsn::new(11));
}
