use wyrd_spec::vala::trace::GenAiEvalResult;

fn eval() -> GenAiEvalResult {
    GenAiEvalResult {
        name: "factuality".into(),
        score_label: Some("pass".into()),
        score_value: Some(0.92),
        explanation: Some("matches grounding documents".into()),
        response_id: Some("resp_abc123".into()),
    }
}

#[test]
fn gen_ai_eval_result_round_trip_full() {
    let result = eval();
    let json = serde_json::to_string(&result).unwrap();
    let back: GenAiEvalResult = serde_json::from_str(&json).unwrap();
    assert_eq!(back, result);
}

#[test]
fn gen_ai_eval_result_round_trip_minimal() {
    let result = GenAiEvalResult {
        name: "safety".into(),
        score_label: None,
        score_value: None,
        explanation: None,
        response_id: None,
    };
    let json = serde_json::to_string(&result).unwrap();
    assert_eq!(json, r#"{"name":"safety"}"#);
    let back: GenAiEvalResult = serde_json::from_str(&json).unwrap();
    assert_eq!(back, result);
}

#[test]
fn gen_ai_eval_result_deny_unknown_fields() {
    let serialized = r#"{"name":"x","rogue":true}"#;
    let result: Result<GenAiEvalResult, _> = serde_json::from_str(serialized);
    assert!(result.is_err());
}

#[test]
fn gen_ai_eval_result_validate_happy_path() {
    eval().validate().unwrap();
}

#[test]
fn gen_ai_eval_result_validate_rejects_empty_name() {
    let result = GenAiEvalResult {
        name: String::new(),
        score_label: None,
        score_value: None,
        explanation: None,
        response_id: None,
    };
    assert!(result.validate().is_err());
}

#[test]
fn gen_ai_eval_result_validate_rejects_overlong_name() {
    let mut result = eval();
    result.name = "a".repeat(257);
    assert!(result.validate().is_err());
}

#[test]
fn gen_ai_eval_result_validate_rejects_nan_score() {
    let mut result = eval();
    result.score_value = Some(f64::NAN);
    assert!(result.validate().is_err());
}

#[test]
fn gen_ai_eval_result_validate_rejects_pos_inf_score() {
    let mut result = eval();
    result.score_value = Some(f64::INFINITY);
    assert!(result.validate().is_err());
}

#[test]
fn gen_ai_eval_result_validate_rejects_neg_inf_score() {
    let mut result = eval();
    result.score_value = Some(f64::NEG_INFINITY);
    assert!(result.validate().is_err());
}

#[test]
fn gen_ai_eval_result_validate_accepts_negative_score() {
    let mut result = eval();
    result.score_value = Some(-3.5);
    result.validate().unwrap();
}

#[test]
fn gen_ai_eval_result_validate_rejects_overlong_score_label() {
    let mut result = eval();
    result.score_label = Some("a".repeat(65));
    assert!(result.validate().is_err());
}

#[test]
fn gen_ai_eval_result_validate_rejects_overlong_explanation() {
    let mut result = eval();
    result.explanation = Some("a".repeat(4097));
    assert!(result.validate().is_err());
}

#[test]
fn gen_ai_eval_result_validate_accepts_4096_char_explanation() {
    let mut result = eval();
    result.explanation = Some("a".repeat(4096));
    result.validate().unwrap();
}

#[test]
fn gen_ai_eval_result_validate_rejects_overlong_response_id() {
    let mut result = eval();
    result.response_id = Some("a".repeat(129));
    assert!(result.validate().is_err());
}
