use std::collections::HashSet;

use wyrd_spec::error::WyrdError;

fn eval_errors() -> Vec<WyrdError> {
    vec![
        WyrdError::EvalRunNotFound {
            message: "eval run abc not found".to_owned(),
            details: serde_json::json!({}),
        },
        WyrdError::EvalMissingLease {
            message: "missing lease token on protected eval-run route".to_owned(),
            details: serde_json::json!({}),
        },
        WyrdError::EvalInvalidLease {
            message: "lease token does not match the lease issued for run abc".to_owned(),
            details: serde_json::json!({}),
        },
        WyrdError::EvalSubmissionMismatch {
            message: "submission rejected: wrong turn".to_owned(),
            details: serde_json::json!({}),
        },
        WyrdError::EvalRunFailed {
            message: "eval run failed: internal engine error".to_owned(),
            details: serde_json::json!({}),
        },
        WyrdError::EvalTooManyRuns {
            message: "too many concurrent eval runs; retry after an existing run completes"
                .to_owned(),
            details: serde_json::json!({}),
        },
    ]
}

#[test]
fn eval_error_codes_are_non_empty_and_have_wyrd_eval_prefix() {
    for err in eval_errors() {
        let problem = err.as_problem_json();
        let code = problem["code"].as_str().expect("code is a string");
        assert!(
            code.starts_with("WYRD_EVAL_"),
            "expected WYRD_EVAL_ prefix, got: {code}"
        );
        assert!(!code.is_empty(), "code must be non-empty");
        let status = problem["status"].as_u64().expect("status is a number");
        assert!(status >= 400, "expected error status, got {status}");
        assert!(
            problem["title"].as_str().is_some_and(|t| !t.is_empty()),
            "title must be non-empty for {code}"
        );
        assert!(
            problem["remediation"]
                .as_str()
                .is_some_and(|r| !r.is_empty()),
            "remediation must be non-empty for {code}"
        );
    }
}

#[test]
fn eval_error_codes_are_unique() {
    let codes: Vec<&'static str> = eval_errors().iter().map(|e| e.code()).collect();
    let unique: HashSet<&str> = codes.iter().copied().collect();
    assert_eq!(
        unique.len(),
        codes.len(),
        "duplicate eval error code detected"
    );
}

#[test]
fn eval_run_not_found_is_404() {
    let err = WyrdError::EvalRunNotFound {
        message: "not found".to_owned(),
        details: serde_json::json!({}),
    };
    assert_eq!(err.code(), "WYRD_EVAL_404_RUN_NOT_FOUND");
    assert_eq!(err.status(), 404);
}

#[test]
fn eval_missing_lease_is_401() {
    let err = WyrdError::EvalMissingLease {
        message: "no lease".to_owned(),
        details: serde_json::json!({}),
    };
    assert_eq!(err.code(), "WYRD_EVAL_401_MISSING_LEASE");
    assert_eq!(err.status(), 401);
}

#[test]
fn eval_invalid_lease_is_403() {
    let err = WyrdError::EvalInvalidLease {
        message: "bad lease".to_owned(),
        details: serde_json::json!({}),
    };
    assert_eq!(err.code(), "WYRD_EVAL_403_INVALID_LEASE");
    assert_eq!(err.status(), 403);
}

#[test]
fn eval_submission_mismatch_is_409() {
    let err = WyrdError::EvalSubmissionMismatch {
        message: "mismatch".to_owned(),
        details: serde_json::json!({}),
    };
    assert_eq!(err.code(), "WYRD_EVAL_409_SUBMISSION_MISMATCH");
    assert_eq!(err.status(), 409);
}

#[test]
fn eval_run_failed_is_500() {
    let err = WyrdError::EvalRunFailed {
        message: "engine died".to_owned(),
        details: serde_json::json!({}),
    };
    assert_eq!(err.code(), "WYRD_EVAL_500_RUN_FAILED");
    assert_eq!(err.status(), 500);
}

#[test]
fn eval_too_many_runs_is_429() {
    let err = WyrdError::EvalTooManyRuns {
        message: "cap reached".to_owned(),
        details: serde_json::json!({}),
    };
    assert_eq!(err.code(), "WYRD_EVAL_429_TOO_MANY_RUNS");
    assert_eq!(err.status(), 429);
}

#[test]
fn dropped_eval_codes_are_absent_from_wyrd_spec() {
    let all_codes: Vec<&str> = eval_errors().iter().map(|e| e.code()).collect();
    assert!(
        !all_codes.contains(&"WYRD_EVAL_401_API_KEY_INVALID"),
        "WYRD_EVAL_401_API_KEY_INVALID must not be present in WyrdError"
    );
    assert!(
        !all_codes.contains(&"WYRD_EVAL_500_RESULTS_PERSISTENCE_FAILED"),
        "WYRD_EVAL_500_RESULTS_PERSISTENCE_FAILED must not be present in WyrdError"
    );
}

#[test]
fn eval_remediation_strings_use_v1_not_api_v1() {
    for err in eval_errors() {
        let remediation = err.remediation();
        assert!(
            !remediation.contains("/api/v1"),
            "remediation for {} must not contain /api/v1, got: {remediation}",
            err.code()
        );
    }
}
