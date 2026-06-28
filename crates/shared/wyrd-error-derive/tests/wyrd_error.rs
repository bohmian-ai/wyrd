use wyrd_error_derive::WyrdError;

#[derive(WyrdError)]
#[allow(dead_code)]
enum ExampleError {
    #[wyrd_error(
        code = "WYRD_TEST_400_VALIDATION",
        status = 400,
        title = "Validation failed",
        remediation = "Fix the submitted test value."
    )]
    Validation { message: String },
    #[wyrd_error(
        code = "WYRD_TEST_500_INTERNAL",
        status = 500,
        title = "Internal failure",
        remediation = "Retry the test request later."
    )]
    Internal,
    #[wyrd_error(delegate)]
    Wrapped { source: InnerError },
}

#[derive(WyrdError)]
#[allow(dead_code)]
enum InnerError {
    #[wyrd_error(
        code = "WYRD_TEST_503_UPSTREAM",
        status = 503,
        title = "Upstream failed",
        remediation = "Retry after the upstream recovers."
    )]
    Upstream,
}

#[derive(WyrdError, Debug, PartialEq)]
#[allow(dead_code)]
enum ReconstructableError {
    #[wyrd_error(
        code = "WYRD_TEST_404_NOT_FOUND",
        status = 404,
        title = "Not found",
        remediation = "Check the id."
    )]
    NotFound { message: String, details: u32 },
    #[wyrd_error(
        code = "WYRD_TEST_409_CONFLICT",
        status = 409,
        title = "Conflict",
        remediation = "Retry."
    )]
    Conflict { message: String, details: u32 },
    // Does not qualify for `from_code` (extra field); must still compile.
    #[wyrd_error(
        code = "WYRD_TEST_500_INTERNAL",
        status = 500,
        title = "Internal",
        remediation = "Retry later."
    )]
    Internal { message: String },
}

#[test]
fn from_code_reconstructs_message_details_variants() {
    let reconstructed =
        ReconstructableError::from_code("WYRD_TEST_404_NOT_FOUND", "gone".to_string(), 7);
    assert_eq!(
        reconstructed,
        Some(ReconstructableError::NotFound {
            message: "gone".to_string(),
            details: 7,
        })
    );
    // The reconstructed typed variant reports its own status, not a catch-all.
    assert_eq!(reconstructed.expect("reconstructs").status(), 404);
}

#[test]
fn from_code_returns_none_for_unknown_or_unqualified_code() {
    assert_eq!(
        ReconstructableError::from_code("WYRD_TEST_999_UNKNOWN", "x".to_string(), 0),
        None,
        "unknown codes have no variant"
    );
    assert_eq!(
        ReconstructableError::from_code("WYRD_TEST_500_INTERNAL", "x".to_string(), 0),
        None,
        "variants without exactly {{message, details}} are not reconstructable"
    );
}

#[test]
fn derives_all_error_metadata_accessors() {
    let validation = ExampleError::Validation {
        message: "bad".to_string(),
    };
    assert_eq!(validation.code(), "WYRD_TEST_400_VALIDATION");
    assert_eq!(validation.status(), 400);
    assert_eq!(validation.title(), "Validation failed");
    assert_eq!(validation.remediation(), "Fix the submitted test value.");

    let internal = ExampleError::Internal;
    assert_eq!(internal.code(), "WYRD_TEST_500_INTERNAL");
    assert_eq!(internal.status(), 500);
    assert_eq!(internal.title(), "Internal failure");
    assert_eq!(internal.remediation(), "Retry the test request later.");

    let wrapped = ExampleError::Wrapped {
        source: InnerError::Upstream,
    };
    assert_eq!(wrapped.code(), "WYRD_TEST_503_UPSTREAM");
    assert_eq!(wrapped.status(), 503);
    assert_eq!(wrapped.title(), "Upstream failed");
    assert_eq!(wrapped.remediation(), "Retry after the upstream recovers.");
}
