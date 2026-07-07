use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::error::BifrostError;

#[test]
fn bifrost_error_delegates_code_and_status_into_wyrd_error() {
    let cases = vec![
        (
            BifrostError::FingerprintMismatch {
                table: "tenant.events".to_owned(),
            },
            "WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH",
            409u16,
        ),
        (
            BifrostError::QueryInvalidSql {
                detail: "not a SELECT".to_owned(),
            },
            "WYRD_VALA_400_QUERY_INVALID_SQL",
            400,
        ),
        (
            BifrostError::QueryResultTooLarge,
            "WYRD_VALA_413_QUERY_RESULT_TOO_LARGE",
            413,
        ),
        (
            BifrostError::QueryTimeout,
            "WYRD_VALA_504_QUERY_TIMEOUT",
            504,
        ),
    ];

    for (wire, code, status) in cases {
        let wire_code = wire.code();
        let wire_status = wire.status();
        let wire_title = wire.title();

        let error: WyrdError = wire.into();

        assert_eq!(error.code(), code);
        assert_eq!(error.status(), status);
        assert_eq!(error.code(), wire_code, "delegate forwards wire code");
        assert_eq!(error.status(), wire_status, "delegate forwards wire status");
        assert_eq!(error.title(), wire_title, "delegate forwards wire title");
    }
}

#[test]
fn bifrost_error_lifts_into_problem_json_without_losing_code() {
    let error: WyrdError = BifrostError::FingerprintMismatch {
        table: "tenant.events".to_owned(),
    }
    .into();

    let problem = error.as_problem_json();

    assert_eq!(
        problem["code"],
        "WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH"
    );
    assert_eq!(problem["status"], 409);
    assert_eq!(problem["title"], "Schema fingerprint mismatch");
    assert_eq!(
        problem["details"]["variant"], "fingerprint_mismatch",
        "vala details retain the wire enum discriminant"
    );
}
