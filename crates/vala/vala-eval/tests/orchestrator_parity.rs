mod orchestrator_support;

use std::collections::VecDeque;
use std::sync::Arc;

use async_trait::async_trait;
use orchestrator_support::{
    assertion_task, eval_ref, fixture_value, judge_task, record, scenario, spec, subject_ref, tid,
};
use serde_json::json;
use tokio::sync::Mutex;
use vala_eval::orchestrator::{
    AgentTurnFn, AgentTurnReply, EmbeddedOrchestrator, OrchestratorError, ScenarioScoring,
};
use vala_eval::{MockJudgeInvoker, SubjectKey};
use wyrd_spec::vala::eval::protocol::{ConversationTurn, SimulatedUserMode};
use wyrd_spec::vala::ids::RunId;

struct ScriptedAgent {
    replies: Mutex<VecDeque<AgentTurnReply>>,
}

impl ScriptedAgent {
    fn new(replies: Vec<AgentTurnReply>) -> Arc<Self> {
        Arc::new(Self {
            replies: Mutex::new(replies.into_iter().collect()),
        })
    }
}

#[async_trait]
impl AgentTurnFn for ScriptedAgent {
    async fn invoke(
        &self,
        _message: &str,
        _history: &[ConversationTurn],
    ) -> Result<AgentTurnReply, OrchestratorError> {
        self.replies
            .lock()
            .await
            .pop_front()
            .ok_or_else(|| OrchestratorError::EmbeddedCallback {
                reason: "scripted agent has no remaining reply".to_owned(),
            })
    }
}

fn reply(
    response: &str,
    records: Vec<wyrd_spec::vala::eval::record::EvalRecordObservation>,
) -> AgentTurnReply {
    AgentTurnReply {
        response: response.to_owned(),
        records,
    }
}

async fn drive(
    tasks: Vec<wyrd_spec::vala::eval::EvalTask>,
    judge_outputs: Vec<Result<serde_json::Value, vala_eval::JudgeError>>,
    scenario: wyrd_spec::vala::eval::EvalScenario,
    replies: Vec<AgentTurnReply>,
) -> vala_eval::orchestrator::EmbeddedOutcome {
    let scoring = ScenarioScoring::with_in_memory_traces(
        Arc::new(spec(tasks)),
        MockJudgeInvoker::new(judge_outputs),
    )
    .expect("scoring builds");
    let orchestrator = EmbeddedOrchestrator {
        scoring,
        simulator: None,
    };

    orchestrator
        .drive(
            eval_ref(),
            SimulatedUserMode::Client,
            vec![scenario],
            ScriptedAgent::new(replies),
            None,
        )
        .await
        .expect("embedded run succeeds")
}

#[tokio::test(flavor = "multi_thread")]
async fn scripted_walk_llm_judge_parity() {
    // parity: scouter/crates/scouter_evaluate/tests/fixtures/single_turn_pass.json
    let fixture = fixture_value(include_str!("fixtures/parity/scripted_judge.json"));
    assert_eq!(fixture["expected_pass_rate"], json!(1.0));
    let run_id = RunId::from_string("scripted-parity-run".to_owned());

    let outcome = drive(
        vec![judge_task("judge_passes")],
        vec![Ok(json!({"passed": true})), Ok(json!({"passed": true}))],
        scenario("scripted_judge", vec!["Continue"], None, 2),
        vec![
            reply("First answer", vec![record(&run_id, true)]),
            reply("DONE", vec![record(&run_id, true)]),
        ],
    )
    .await;

    assert_eq!(outcome.scenarios.len(), 1);
    assert_eq!(outcome.scenarios[0].mechanic.len(), 2);
    assert!(
        outcome.scenarios[0]
            .mechanic
            .iter()
            .all(|row| row.result.passed)
    );
    assert!(outcome.scenarios[0].passenger.iter().all(|row| row.passed));
    assert!((outcome.run.metrics.pass_rate - 1.0).abs() < f64::EPSILON);
}

#[tokio::test(flavor = "multi_thread")]
async fn termination_signal_parity() {
    // parity: scouter/crates/scouter_evaluate/tests/fixtures/termination_signal.json
    let fixture = fixture_value(include_str!("fixtures/parity/termination_signal.json"));
    assert_eq!(fixture["expected_scenario_passed"], json!(true));
    let run_id = RunId::from_string("signal-parity-run".to_owned());

    let outcome = drive(
        vec![assertion_task("context_ok")],
        Vec::new(),
        scenario("termination_signal", Vec::new(), Some("DONE"), 8),
        vec![reply("DONE", vec![record(&run_id, true)])],
    )
    .await;

    assert_eq!(outcome.scenarios[0].mechanic.len(), 1);
    assert!(outcome.scenarios[0].mechanic[0].result.passed);
    assert!(
        outcome
            .run
            .scenarios
            .values()
            .all(|scenario| scenario.scenario_passed)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn subject_aggregation_parity() {
    // parity: scouter/crates/scouter_evaluate/tests/fixtures/subject_rollup.json
    let fixture = fixture_value(include_str!("fixtures/parity/subject_rollup.json"));
    assert_eq!(fixture["expected_subject_pass_rate"], json!(1.0));
    let run_id = RunId::from_string("subject-parity-run".to_owned());

    let outcome = drive(
        vec![assertion_task("subject_context_ok")],
        Vec::new(),
        scenario("subject_rollup", Vec::new(), Some("DONE"), 1),
        vec![reply("DONE", vec![record(&run_id, true)])],
    )
    .await;

    let key = SubjectKey::from_ref(&subject_ref());
    assert_eq!(outcome.run.subjects.len(), 1);
    assert_eq!(outcome.run.subjects[&key].metrics.total_tasks, 1);
    assert!((outcome.run.subjects[&key].metrics.pass_rate - 1.0).abs() < f64::EPSILON);
    assert_eq!(
        outcome.run.metrics.per_task_pass_rate[&tid("subject_context_ok")],
        1.0
    );
}
