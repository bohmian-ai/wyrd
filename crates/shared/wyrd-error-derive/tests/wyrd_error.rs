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
}
