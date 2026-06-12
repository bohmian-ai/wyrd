use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardUid, DataTenantId, SpaceName, TenantSlug};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::trace::TraceContext;
use wyrd_spec::version::{VersionBlock, VersionBump, VersionRange};

#[test]
fn ids_enforce_type_specific_canonical_forms() {
    assert!(SpaceName::new("prod_space").is_ok());
    assert!(SpaceName::new("ProdSpace").is_err());
    assert!(SpaceName::new("ab").is_err());

    assert!(CardUid::new("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00").is_ok());
    assert!(CardUid::new("550e8400-e29b-41d4-a716-446655440000").is_err());

    assert!(TenantSlug::new("a").is_ok());
    assert!(TenantSlug::new("0_acme-corp").is_ok());
    assert!(TenantSlug::new("Acme").is_err());
    assert!(TenantSlug::new("acme.corp").is_err());
}

#[test]
fn request_id_accepts_ulid_or_uuid7_only() {
    assert!(RequestId::parse("01ARZ3NDEKTSV4RRFFQ69G5FAV").is_ok());
    assert!(RequestId::parse("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00").is_ok());
    assert!(RequestId::parse("550e8400-e29b-41d4-a716-446655440000").is_err());
}

#[test]
fn data_tenant_id_is_uuid7_backed() {
    let generated = DataTenantId::new_v7();
    assert_eq!(generated.as_uuid().get_version_num(), 7);

    let parsed: DataTenantId = generated.to_string().parse().unwrap();
    assert_eq!(parsed, generated);
    assert!(DataTenantId::new(uuid::Uuid::nil()).is_err());
    assert!(
        "550e8400-e29b-41d4-a716-446655440000"
            .parse::<DataTenantId>()
            .is_err()
    );
}

#[test]
fn traceparent_validation_is_w3c_canonical() {
    let context = TraceContext::parse_traceparent(
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        String::new(),
    )
    .unwrap();
    assert_eq!(
        context.to_traceparent(),
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
    );
    assert!(
        TraceContext::parse_traceparent(
            "00-4BF92F3577B34DA6A3CE929D0E0E4736-00f067aa0ba902b7-01",
            String::new(),
        )
        .is_err()
    );
    assert!(
        TraceContext::parse_traceparent(
            "00-00000000000000000000000000000000-00f067aa0ba902b7-01",
            String::new(),
        )
        .is_err()
    );
}

#[test]
fn version_helpers_match_bump_and_sort() {
    let range = VersionRange::parse("^1.2").unwrap();
    assert!(
        range
            .matches(&VersionBlock::parse("1.4.0").unwrap())
            .unwrap()
    );
    assert!(
        !range
            .matches(&VersionBlock::parse("2.0.0").unwrap())
            .unwrap()
    );

    let version = VersionBlock::parse("1.2.3").unwrap();
    assert_eq!(version.bump(VersionBump::Minor).unwrap().as_str(), "1.3.0");

    let mut versions = vec![
        VersionBlock::parse("1.10.0").unwrap(),
        VersionBlock::parse("1.2.0").unwrap(),
    ];
    VersionBlock::sort_versions(&mut versions).unwrap();
    assert_eq!(versions[0].as_str(), "1.2.0");
}

#[test]
fn foundation_error_has_stable_problem_payload() {
    let error = WyrdError::Validation {
        message: "bad card".to_string(),
        details: serde_json::json!({ "field": "kind" }),
    };
    let problem = error.as_problem_json();
    assert_eq!(error.code(), "WYRD_SPEC_400_VALIDATION");
    assert_eq!(problem["status"], 400);
    assert_eq!(problem["code"], "WYRD_SPEC_400_VALIDATION");
    assert!(problem["remediation"].as_str().unwrap().contains("schema"));
}
