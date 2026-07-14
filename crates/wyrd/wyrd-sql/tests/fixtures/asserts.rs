//! Named assertion helpers for the cards e2e suite.
#![allow(dead_code)]

use wyrd_semver::VersionBlock;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;
use wyrd_spec::vala::audit_detail::CardRegistrationOutcome;
use wyrd_sql::queries::cards::{ListPage, RegisterCardOutcome, RegisterCardOutcomeKind};
use wyrd_sql::{CardRow, ParsedCardRow};

use super::scenarios::RegistrationAuditRow;

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

pub fn assert_deduplicated(o: &RegisterCardOutcome) {
    assert_eq!(
        o.kind,
        RegisterCardOutcomeKind::Deduplicated,
        "expected Deduplicated outcome, got {:?}",
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

pub fn assert_version(row: &ParsedCardRow, expected: &str) {
    assert_eq!(
        row.version.as_str(),
        expected,
        "expected version {expected}, got {}",
        row.version
    );
}

pub fn assert_versions_eq(got: &[VersionBlock], expected: &[&str]) {
    let got = got.iter().map(VersionBlock::as_str).collect::<Vec<_>>();
    assert_eq!(got, expected, "version list mismatch");
}

pub fn assert_no_prerelease(row: &ParsedCardRow) {
    let semver = row.version.semver().expect("stored version is semver");
    assert!(
        semver.pre.is_empty(),
        "expected stable version, got {}",
        row.version
    );
}

pub fn assert_page_uids(page: &ListPage<CardRow>, expected_len: usize) {
    assert_eq!(page.items.len(), expected_len, "page length mismatch");
    let mut seen = std::collections::BTreeSet::new();
    for row in &page.items {
        assert!(seen.insert(row.card_uid), "duplicate uid in page");
    }
}

pub fn assert_registration_outcome(row: &RegistrationAuditRow, expected: CardRegistrationOutcome) {
    let detail = row
        .detail
        .as_deref()
        .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok());
    let actual = detail
        .as_ref()
        .and_then(|value| value.get("outcome"))
        .and_then(serde_json::Value::as_str);
    let expected =
        serde_json::to_value(expected).expect("card registration outcome is JSON serializable");
    assert_eq!(
        actual,
        expected.as_str(),
        "registration audit outcome mismatch"
    );
}

pub fn assert_card_registration_detail(row: &RegistrationAuditRow, uid: &CardUid) {
    let detail = row
        .detail
        .as_deref()
        .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
        .expect("audit row has valid typed detail");
    let expected_uid = uid.to_string();
    assert_eq!(detail["kind"], "card_registration");
    assert_eq!(detail["card_uid"].as_str(), Some(expected_uid.as_str()));
}

pub fn assert_card_registration_operation(row: &RegistrationAuditRow, expected: &str) {
    let detail = row
        .detail
        .as_deref()
        .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
        .expect("audit row has valid typed detail");
    assert_eq!(detail["operation"].as_str(), Some(expected));
}
