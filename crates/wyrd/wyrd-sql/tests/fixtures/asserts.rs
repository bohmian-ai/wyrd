//! Named assertion helpers for the cards e2e suite.
#![allow(dead_code)]

use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;
use wyrd_sql::queries::cards::{ListPage, RegisterCardOutcome, RegisterCardOutcomeKind};

pub fn assert_created(o: &RegisterCardOutcome) {
    assert_eq!(
        o.kind,
        RegisterCardOutcomeKind::Created,
        "expected Created outcome, got {:?}",
        o.kind
    );
}

pub fn assert_idempotent_noop(o: &RegisterCardOutcome) {
    assert_eq!(
        o.kind,
        RegisterCardOutcomeKind::IdempotentNoop,
        "expected IdempotentNoop outcome, got {:?}",
        o.kind
    );
}

pub fn assert_uuid_v7(uid: &CardUid) {
    let uuid = uid.as_uuid();
    assert_eq!(uuid.get_version_num(), 7, "card_uid {uid} is not UUIDv7");
}

pub fn assert_no_principal(o: &RegisterCardOutcome) {
    assert!(
        o.principal_id.is_none(),
        "expected no principal_id, got {:?}",
        o.principal_id
    );
}

pub fn assert_principal_projected(o: &RegisterCardOutcome) {
    assert!(o.principal_id.is_some(), "expected principal_id to be set");
}

pub fn assert_same_uid(a: &RegisterCardOutcome, b: &RegisterCardOutcome) {
    assert_eq!(
        a.card_uid, b.card_uid,
        "expected same card_uid across re-apply"
    );
}

pub fn assert_distinct_uids(a: &RegisterCardOutcome, b: &RegisterCardOutcome) {
    assert_ne!(
        a.card_uid, b.card_uid,
        "expected distinct card_uids for version bumps"
    );
}

pub fn assert_same_principal(a: &RegisterCardOutcome, b: &RegisterCardOutcome) {
    assert_eq!(
        a.principal_id, b.principal_id,
        "expected principal_id preserved on re-apply"
    );
}

pub fn assert_distinct_principals(a: &RegisterCardOutcome, b: &RegisterCardOutcome) {
    assert_ne!(
        a.principal_id, b.principal_id,
        "expected distinct principals for version bumps"
    );
}

pub fn assert_error_code(err: &WyrdError, expected_code: &str) {
    let actual = err.code();
    assert_eq!(
        actual, expected_code,
        "expected error code {expected_code:?}, got {actual:?}"
    );
}

pub fn assert_page_size<T>(page: &ListPage<T>, expected: usize) {
    assert_eq!(
        page.items.len(),
        expected,
        "expected page of {expected} items, got {}",
        page.items.len()
    );
}
