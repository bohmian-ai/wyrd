use wyrd_spec::error::WyrdError;

fn registry_errors() -> Vec<WyrdError> {
    vec![
        WyrdError::RegistryInvalidCardSpec {
            message: "unknown kind".to_owned(),
            details: serde_json::json!({}),
        },
        WyrdError::RegistryInvalidVersionBlock {
            message: "service cards require a pin version".to_owned(),
            details: serde_json::json!({}),
        },
        WyrdError::RegistrySpecTooLarge {
            message: "spec exceeds 256 KiB".to_owned(),
            details: serde_json::json!({}),
        },
        WyrdError::RegistryVersionRequired {
            message: "metadata.version is required".to_owned(),
            details: serde_json::json!({}),
        },
        WyrdError::RegistryListLimitOutOfRange {
            message: "limit out of range".to_owned(),
            details: serde_json::json!({}),
        },
        WyrdError::RegistryCardRefUidNotResolvableHere {
            message: "uid not resolvable here".to_owned(),
            details: serde_json::json!({}),
        },
        WyrdError::RegistryRequirementNotResolvableHere {
            message: "requirement version not accepted here".to_owned(),
            details: serde_json::json!({}),
        },
        WyrdError::RegistryCardNotFound {
            message: "card not found".to_owned(),
            details: serde_json::json!({}),
        },
        WyrdError::RegistryVersionConflict {
            message: "version conflict".to_owned(),
            details: serde_json::json!({}),
        },
        WyrdError::RegistrySpecDrift {
            message: "spec changed for immutable version".to_owned(),
            details: serde_json::json!({}),
        },
        WyrdError::RegistryUnavailable {
            message: "registry unavailable".to_owned(),
            details: serde_json::json!({}),
        },
        WyrdError::PrincipalOrphaned {
            message: "backing card deleted".to_owned(),
            details: serde_json::json!({}),
        },
    ]
}

#[test]
fn every_registry_variant_has_problem_json_round_trip() {
    for err in registry_errors() {
        let problem = err.as_problem_json();
        let code = problem["code"].as_str().expect("code is a string");
        assert!(
            code.starts_with("WYRD_REG_") || code.starts_with("WYRD_AUTH_"),
            "unexpected code prefix: {code}"
        );
        assert!(problem["status"].as_u64().unwrap() >= 400);
        assert!(problem["title"].as_str().is_some());
    }
}

#[test]
fn registry_unavailable_status_is_503() {
    let err = WyrdError::RegistryUnavailable {
        message: "transient".to_owned(),
        details: serde_json::json!({}),
    };
    let problem = err.as_problem_json();
    assert_eq!(problem["status"], 503);
    assert_eq!(problem["code"], "WYRD_REG_503_REGISTRY_UNAVAILABLE");
}

#[test]
fn spec_drift_status_is_409() {
    let err = WyrdError::RegistrySpecDrift {
        message: "drift".to_owned(),
        details: serde_json::json!({}),
    };
    let problem = err.as_problem_json();
    assert_eq!(problem["status"], 409);
    assert_eq!(problem["code"], "WYRD_REG_409_SPEC_DRIFT");
}

#[test]
fn registry_card_not_found_status_is_404() {
    let err = WyrdError::RegistryCardNotFound {
        message: "not found".to_owned(),
        details: serde_json::json!({}),
    };
    let problem = err.as_problem_json();
    assert_eq!(problem["status"], 404);
    assert_eq!(problem["code"], "WYRD_REG_404_CARD_NOT_FOUND");
}

#[test]
fn registry_400_variants_all_return_status_400() {
    let cases = vec![
        WyrdError::RegistryInvalidCardSpec {
            message: "bad spec".to_owned(),
            details: serde_json::json!({}),
        },
        WyrdError::RegistryInvalidVersionBlock {
            message: "bad version".to_owned(),
            details: serde_json::json!({}),
        },
        WyrdError::RegistrySpecTooLarge {
            message: "too large".to_owned(),
            details: serde_json::json!({}),
        },
        WyrdError::RegistryVersionRequired {
            message: "version required".to_owned(),
            details: serde_json::json!({}),
        },
        WyrdError::RegistryListLimitOutOfRange {
            message: "limit out of range".to_owned(),
            details: serde_json::json!({}),
        },
    ];
    for err in cases {
        let problem = err.as_problem_json();
        assert_eq!(
            problem["status"], 400,
            "expected status 400 for {}",
            problem["code"]
        );
    }
}

#[test]
fn registry_version_conflict_status_is_500() {
    let err = WyrdError::RegistryVersionConflict {
        message: "uid mismatch".to_owned(),
        details: serde_json::json!({}),
    };
    let problem = err.as_problem_json();
    assert_eq!(problem["status"], 500);
    assert_eq!(problem["code"], "WYRD_REG_500_VERSION_CONFLICT");
}

#[test]
fn principal_orphaned_status_matches_embedded_code() {
    let err = WyrdError::PrincipalOrphaned {
        message: "card deleted".to_owned(),
        details: serde_json::json!({}),
    };
    let problem = err.as_problem_json();
    let code = problem["code"].as_str().expect("code is a string");
    let status = problem["status"].as_u64().expect("status is a number");
    let code_status: u64 = code
        .split('_')
        .find(|s| s.chars().all(|c| c.is_ascii_digit()))
        .and_then(|s| s.parse().ok())
        .expect("code must embed a numeric status");
    assert_eq!(code_status, status);
}
