mod eval_support;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use serde_json::Value;
use wyrd_spec::vala::eval::EvalPassGate;

use eval_support::{assertion_task, judge_task, record, spec, write_eval_card, write_records};

#[test]
fn local_records_run_scores_fixture_records() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let eval_path = tmp.path().join("eval.json");
    let records_path = tmp.path().join("records.jsonl");
    let out_dir = tmp.path().join("out");
    write_eval_card(
        &eval_path,
        spec(
            vec![assertion_task(), judge_task()],
            Some(EvalPassGate::AllPass),
        ),
    );
    write_records(&records_path, &[record(true)]);

    std::process::Command::cargo_bin("wyrd")
        .expect("wyrd binary")
        .args([
            "eval",
            "run",
            "--eval",
            eval_path.to_str().expect("utf8 path"),
            "--records",
            records_path.to_str().expect("utf8 path"),
            "--judge-mock",
            "--out",
            out_dir.to_str().expect("utf8 path"),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("pass_gate: PASS"));

    let raw = std::fs::read_to_string(out_dir.join("SUMMARY.json")).expect("summary reads");
    let summary: Value = serde_json::from_str(&raw).expect("summary json");
    assert_eq!(summary["metrics"]["total_tasks"], 2);
    assert_eq!(summary["metrics"]["passed_tasks"], 2);
    assert_eq!(summary["pass_gate_verdict"]["passed"], true);
}

#[test]
fn local_records_run_exits_two_when_gate_fails() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let eval_path = tmp.path().join("eval.json");
    let records_path = tmp.path().join("records.jsonl");
    let out_dir = tmp.path().join("out");
    write_eval_card(
        &eval_path,
        spec(vec![assertion_task()], Some(EvalPassGate::AllPass)),
    );
    write_records(&records_path, &[record(false)]);

    std::process::Command::cargo_bin("wyrd")
        .expect("wyrd binary")
        .args([
            "eval",
            "run",
            "--eval",
            eval_path.to_str().expect("utf8 path"),
            "--records",
            records_path.to_str().expect("utf8 path"),
            "--judge-mock",
            "--out",
            out_dir.to_str().expect("utf8 path"),
        ])
        .assert()
        .code(2);
}
