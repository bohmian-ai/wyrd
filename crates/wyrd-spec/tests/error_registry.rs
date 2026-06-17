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
