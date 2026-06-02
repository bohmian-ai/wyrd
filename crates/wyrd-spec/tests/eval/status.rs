use wyrd_spec::vala::eval::EvalStatus;

#[test]
fn eval_status_round_trips_snake_case() {
    let json = serde_json::to_string(&EvalStatus::AwaitingTrace).expect("status serializes");
    assert_eq!(json, "\"awaiting_trace\"");
    let round_trip: EvalStatus = serde_json::from_str(&json).expect("status deserializes");
    assert_eq!(round_trip, EvalStatus::AwaitingTrace);
}

#[test]
fn eval_status_rejects_unknown_variant() {
    let result: Result<EvalStatus, _> = serde_json::from_str("\"unknown\"");
    assert!(result.is_err());
}
