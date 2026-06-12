use wyrd_spec::vala::eval::result::EvalPassGate;

#[test]
fn overall_pass_rate_round_trip() {
    let g = EvalPassGate::OverallPassRate { threshold: 0.9 };
    let s = serde_json::to_string(&g).unwrap();
    assert_eq!(s, r#"{"kind":"overall_pass_rate","threshold":0.9}"#);
    let back: EvalPassGate = serde_json::from_str(&s).unwrap();
    assert_eq!(g, back);
}

#[test]
fn per_judge_pass_rate_round_trip() {
    let g = EvalPassGate::PerJudgePassRate { threshold: 0.75 };
    let s = serde_json::to_string(&g).unwrap();
    let back: EvalPassGate = serde_json::from_str(&s).unwrap();
    assert_eq!(g, back);
}

#[test]
fn all_pass_round_trip() {
    let g = EvalPassGate::AllPass;
    let s = serde_json::to_string(&g).unwrap();
    assert_eq!(s, r#"{"kind":"all_pass"}"#);
    let back: EvalPassGate = serde_json::from_str(&s).unwrap();
    assert_eq!(g, back);
}

#[test]
fn validate_accepts_in_range() {
    EvalPassGate::OverallPassRate { threshold: 0.0 }
        .validate()
        .unwrap();
    EvalPassGate::OverallPassRate { threshold: 1.0 }
        .validate()
        .unwrap();
    EvalPassGate::OverallPassRate { threshold: 0.5 }
        .validate()
        .unwrap();
    EvalPassGate::AllPass.validate().unwrap();
}

#[test]
fn validate_rejects_out_of_range() {
    assert!(
        EvalPassGate::OverallPassRate { threshold: -0.1 }
            .validate()
            .is_err()
    );
    assert!(
        EvalPassGate::OverallPassRate { threshold: 1.1 }
            .validate()
            .is_err()
    );
    assert!(
        EvalPassGate::PerJudgePassRate {
            threshold: f64::NAN,
        }
        .validate()
        .is_err()
    );
}
