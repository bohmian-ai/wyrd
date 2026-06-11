//! Confirms the crate compiles and the `lib.rs` surface re-exports are visible.
//! Later commits replace the stubs with real bodies.

use vala_eval::{EvalError, EvalExecError};

#[test]
fn public_surface_is_visible() {
    let _ = std::any::type_name::<EvalError>();
    let _ = std::any::type_name::<EvalExecError>();
    let _ = std::any::type_name::<vala_eval::context::ExecutionContext>();
}

#[test]
fn eval_error_carries_wyrd_code() {
    let error = EvalError::JudgeUnavailable {
        message: "mock provider down".to_owned(),
        details: serde_json::json!({ "reason": "mock provider down" }),
    };
    assert_eq!(error.code(), "WYRD_EVAL_503_JUDGE_UNAVAILABLE");
    assert_eq!(error.status(), 503);
}

#[test]
fn exec_error_translates_into_eval_error() {
    let exec = EvalExecError::JsonPathFailure {
        path: "$.outputs.tool_calls".to_owned(),
        reason: "no match".to_owned(),
    };
    let public: EvalError = exec.into();
    let problem = public.as_problem_json();
    assert_eq!(problem["code"], "WYRD_EVAL_422_JSONPATH_FAILURE");
    assert_eq!(problem["details"]["path"], "$.outputs.tool_calls");
}
