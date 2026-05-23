use serde_json::json;
use wyrd_spec::run::RunKind;

#[test]
fn remediation_run_kind_round_trips() {
    let encoded = serde_json::to_value(RunKind::Remediation).unwrap();
    assert_eq!(encoded, json!({"type": "remediation"}));

    let decoded: RunKind = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, RunKind::Remediation);
}
