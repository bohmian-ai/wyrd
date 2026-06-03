use wyrd_spec::vala::eval::{
    EntityUid, JsonPath, RecordId, ScenarioId, SessionId, SpanId, TaskId, TraceId, WorkflowUid,
};

#[test]
fn task_id_accepts_simple_identifier() {
    let id = TaskId::new("step_one").expect("valid task id");
    assert_eq!(id.as_str(), "step_one");
}

#[test]
fn task_id_accepts_hyphen_after_first_char() {
    TaskId::new("step-one").expect("valid task id");
}

#[test]
fn task_id_rejects_leading_digit() {
    assert!(TaskId::new("1step").is_err());
}

#[test]
fn task_id_rejects_empty() {
    assert!(TaskId::new("").is_err());
}

#[test]
fn task_id_rejects_overlong() {
    let long = "a".repeat(129);
    assert!(TaskId::new(long).is_err());
}

#[test]
fn task_id_serde_transparent() {
    let id = TaskId::new("alpha").expect("valid task id");
    let json = serde_json::to_string(&id).expect("task id serializes");
    assert_eq!(json, "\"alpha\"");
    let round_trip: TaskId = serde_json::from_str(&json).expect("task id deserializes");
    assert_eq!(id, round_trip);
}

#[test]
fn task_id_deserialize_rejects_leading_digit() {
    let result: Result<TaskId, _> = serde_json::from_str("\"1step\"");
    assert!(result.is_err());
}

#[test]
fn task_id_deserialize_rejects_overlong() {
    let long = format!("\"{}\"", "a".repeat(129));
    let result: Result<TaskId, _> = serde_json::from_str(&long);
    assert!(result.is_err());
}

#[test]
fn task_id_deserialize_rejects_empty_string() {
    let result: Result<TaskId, _> = serde_json::from_str("\"\"");
    assert!(result.is_err());
}

#[test]
fn task_id_deserialize_rejects_disallowed_char() {
    let result: Result<TaskId, _> = serde_json::from_str("\"has space\"");
    assert!(result.is_err());
}

#[test]
fn scenario_id_accepts_simple_identifier() {
    let id = ScenarioId::new("scenario_one").expect("valid scenario id");
    assert_eq!(id.as_str(), "scenario_one");
}

#[test]
fn scenario_id_rejects_leading_digit() {
    assert!(ScenarioId::new("1scenario").is_err());
}

#[test]
fn scenario_id_rejects_empty() {
    assert!(ScenarioId::new("").is_err());
}

#[test]
fn scenario_id_rejects_overlong() {
    let long = "a".repeat(129);
    assert!(ScenarioId::new(long).is_err());
}

#[test]
fn scenario_id_serde_transparent() {
    let id = ScenarioId::new("alpha").expect("valid scenario id");
    let json = serde_json::to_string(&id).expect("scenario id serializes");
    assert_eq!(json, "\"alpha\"");
    let round_trip: ScenarioId = serde_json::from_str(&json).expect("scenario id deserializes");
    assert_eq!(id, round_trip);
}

#[test]
fn scenario_id_deserialize_rejects_leading_digit() {
    let result: Result<ScenarioId, _> = serde_json::from_str("\"1scenario\"");
    assert!(result.is_err());
}

#[test]
fn scenario_id_deserialize_rejects_overlong() {
    let long = format!("\"{}\"", "a".repeat(129));
    let result: Result<ScenarioId, _> = serde_json::from_str(&long);
    assert!(result.is_err());
}

#[test]
fn scenario_id_deserialize_rejects_empty_string() {
    let result: Result<ScenarioId, _> = serde_json::from_str("\"\"");
    assert!(result.is_err());
}

#[test]
fn json_path_accepts_root() {
    JsonPath::new("$").expect("valid json path");
}

#[test]
fn json_path_accepts_dot_segments() {
    JsonPath::new("$.response.message").expect("valid json path");
}

#[test]
fn json_path_accepts_bracket_indices() {
    JsonPath::new("$.docs[0].title").expect("valid json path");
}

#[test]
fn json_path_accepts_quoted_keys() {
    JsonPath::new("$['weird key'].value").expect("valid json path");
}

#[test]
fn json_path_rejects_missing_dollar() {
    assert!(JsonPath::new("response").is_err());
}

#[test]
fn json_path_rejects_unbalanced_bracket() {
    assert!(JsonPath::new("$.docs[0").is_err());
}

#[test]
fn json_path_rejects_unbalanced_quote() {
    assert!(JsonPath::new("$['unterminated").is_err());
}

#[test]
fn json_path_rejects_control_char() {
    assert!(JsonPath::new("$.a\nb").is_err());
}

#[test]
fn json_path_deserialize_rejects_missing_dollar() {
    let result: Result<JsonPath, _> = serde_json::from_str("\"response\"");
    assert!(result.is_err());
}

#[test]
fn json_path_deserialize_rejects_unbalanced_bracket() {
    let result: Result<JsonPath, _> = serde_json::from_str("\"$.docs[0\"");
    assert!(result.is_err());
}

#[test]
fn json_path_accepts_key_ending_in_escaped_backslash() {
    // $['a\\'] means the key "a\" (backslash at end).
    // Old code rejected this: the second `\` was `previous`, so it mistakenly
    // treated the closing `'` as escaped and never closed in_single_quote.
    // New code (escaped boolean) correctly toggles in_single_quote on the `'`.
    JsonPath::new(r"$['a\\']").expect("key ending with escaped backslash is valid");
}

#[test]
fn json_path_accepts_escaped_backslash_then_bracket() {
    JsonPath::new(r"$['a\\'][0]").expect("escaped backslash then bracket is valid");
}

#[test]
fn json_path_deserialize_rejects_unbalanced_quote() {
    let result: Result<JsonPath, _> = serde_json::from_str("\"$['unterminated\"");
    assert!(result.is_err());
}

#[test]
fn json_path_deserialize_rejects_control_char() {
    let result: Result<JsonPath, _> = serde_json::from_str("\"$.a\\nb\"");
    assert!(result.is_err());
}

#[test]
fn id_uuid_types_serde_round_trip() {
    let uuid = uuid::Uuid::nil();
    let session = SessionId(uuid);
    let record = RecordId(uuid);
    let workflow = WorkflowUid(uuid);

    assert_eq!(
        serde_json::to_string(&session).expect("session id serializes"),
        format!("\"{uuid}\"")
    );
    assert_eq!(
        session,
        serde_json::from_str(&format!("\"{uuid}\"")).expect("session id")
    );
    assert_eq!(
        record,
        serde_json::from_str(&format!("\"{uuid}\"")).expect("record id")
    );
    assert_eq!(
        workflow,
        serde_json::from_str(&format!("\"{uuid}\"")).expect("workflow id")
    );
}

#[test]
fn entity_uid_serde_round_trip() {
    let entity = EntityUid("customer-123".to_string());
    let json = serde_json::to_string(&entity).expect("entity uid serializes");
    assert_eq!(json, "\"customer-123\"");
    let round_trip: EntityUid = serde_json::from_str(&json).expect("entity uid deserializes");
    assert_eq!(entity, round_trip);
}

#[test]
fn trace_id_serde_byte_array() {
    let trace = TraceId([0; 16]);
    let json = serde_json::to_string(&trace).expect("trace id serializes");
    let value: serde_json::Value = serde_json::from_str(&json).expect("trace id json parses");
    assert_eq!(value.as_array().map(std::vec::Vec::len), Some(16));
}

#[test]
fn span_id_serde_byte_array() {
    let span = SpanId([0; 8]);
    let json = serde_json::to_string(&span).expect("span id serializes");
    let value: serde_json::Value = serde_json::from_str(&json).expect("span id json parses");
    assert_eq!(value.as_array().map(std::vec::Vec::len), Some(8));
}
