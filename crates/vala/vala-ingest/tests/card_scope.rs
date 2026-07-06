//! The card-scope anti-forgery gate (no DB required): `validate_card_scope`
//! authorizes every per-row `card_ref` against the principal's scope.

use std::str::FromStr;
use std::sync::Arc;

use arrow::array::{RecordBatch, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use vala_ingest::error::IngestError;
use vala_ingest::orchestrator::validate_card_scope;
use wyrd_runtime::{Permission, PermissionSet, Principal, PrincipalId, PrincipalKind};
use wyrd_spec::DataTenantId;
use wyrd_spec::reference::{CardRef, CardRefScope};

const IN_SCOPE: &str = "prod/Service/billing@1.0.0";
const OUT_OF_SCOPE: &str = "prod/Service/shipping@1.0.0";

fn card(canonical: &str) -> CardRef {
    CardRef::from_str(canonical).expect("canonical card ref parses")
}

fn principal_with_scope(cards: &[&str]) -> Principal {
    let card_ref = card(IN_SCOPE);
    let card_ref_scope =
        CardRefScope::from_root_and_members(&card_ref, cards.iter().map(|c| card(c)));
    Principal::new(
        PrincipalId::new(uuid::Uuid::now_v7()),
        PrincipalKind::Service {
            card_ref,
            card_ref_scope,
        },
        DataTenantId::new_v7(),
        Vec::new(),
        PermissionSet::from_iter([Permission::bifrost_record_write()]),
    )
}

fn batch_with_card_refs(values: Vec<Option<&str>>) -> RecordBatch {
    let schema = Arc::new(Schema::new(vec![Field::new(
        "card_ref",
        DataType::Utf8,
        true,
    )]));
    let column = Arc::new(StringArray::from(values)) as Arc<dyn arrow::array::Array>;
    RecordBatch::try_new(schema, vec![column]).expect("batch builds")
}

fn batch_without_card_ref() -> RecordBatch {
    let schema = Arc::new(Schema::new(vec![Field::new("val", DataType::Int64, false)]));
    let column =
        Arc::new(arrow::array::Int64Array::from(vec![1_i64, 2])) as Arc<dyn arrow::array::Array>;
    RecordBatch::try_new(schema, vec![column]).expect("batch builds")
}

#[test]
fn card_scope_accepts_all_in_scope_rows() {
    let principal = principal_with_scope(&[IN_SCOPE]);
    let batch = batch_with_card_refs(vec![Some(IN_SCOPE), Some(IN_SCOPE)]);

    validate_card_scope(&[batch], &principal).expect("all in-scope cards pass");
}

#[test]
fn card_scope_rejects_out_of_scope_card() {
    let principal = principal_with_scope(&[IN_SCOPE]);
    let batch = batch_with_card_refs(vec![Some(IN_SCOPE), Some(OUT_OF_SCOPE)]);

    let err = validate_card_scope(&[batch], &principal).expect_err("out-of-scope card rejects");
    assert!(matches!(err, IngestError::CardScopeDenied { .. }));
    assert_eq!(err.wyrd_code(), "WYRD_VALA_403_BIFROST_CARD_SCOPE");
}

#[test]
fn card_scope_rejects_null_card() {
    let principal = principal_with_scope(&[IN_SCOPE]);
    let batch = batch_with_card_refs(vec![Some(IN_SCOPE), None]);

    let err = validate_card_scope(&[batch], &principal).expect_err("null card rejects");
    assert!(matches!(err, IngestError::CardScopeDenied { .. }));
}

#[test]
fn card_scope_rejects_absent_column() {
    let principal = principal_with_scope(&[IN_SCOPE]);

    let err =
        validate_card_scope(&[batch_without_card_ref()], &principal).expect_err("absent rejects");
    assert!(matches!(err, IngestError::CardScopeDenied { .. }));
}

#[test]
fn card_scope_rejects_unparseable_card() {
    let principal = principal_with_scope(&[IN_SCOPE]);
    let batch = batch_with_card_refs(vec![Some("not-a-card-ref")]);

    let err = validate_card_scope(&[batch], &principal).expect_err("garbage rejects");
    assert!(matches!(err, IngestError::CardScopeDenied { .. }));
}

#[test]
fn card_scope_empty_user_principal_cannot_write() {
    // A User principal has an empty card scope, so no supplied card is in scope.
    let user = Principal::new(
        PrincipalId::new(uuid::Uuid::now_v7()),
        PrincipalKind::User,
        DataTenantId::new_v7(),
        Vec::new(),
        PermissionSet::from_iter([Permission::bifrost_record_write()]),
    );
    let batch = batch_with_card_refs(vec![Some(IN_SCOPE)]);

    let err = validate_card_scope(&[batch], &user).expect_err("empty scope rejects all");
    assert!(matches!(err, IngestError::CardScopeDenied { .. }));
}
