mod orchestrator_support;

use std::collections::VecDeque;
use std::sync::Arc;

use chrono::Utc;
use orchestrator_support::{
    assertion_task, eval_ref, fixture_value, judge_task, record, scenario, spec, subject_ref, tid,
};
use serde_json::json;
use tokio::sync::Mutex;
use vala_eval::orchestrator::{NextDirective, RunState, ScenarioScoring};
use vala_eval::{
    EvalResults, JudgeError, MockJudgeInvoker, RunIdentity, ScenarioAggregationInput,
    ScenarioExecutionResults, SubjectKey,
};
use wyrd_spec::vala::eval::protocol::{AgentTurnSubmission, SimulatedUserMode, TurnDirective};
use wyrd_spec::vala::eval::record::EvalRecordObservation;
use wyrd_spec::vala::eval::{EvalScenario, EvalTask};

struct ScriptedAgent {
    replies: Mutex<VecDeque<(String, Vec<EvalRecordObservation>)>>,
}

impl ScriptedAgent {
    fn new(replies: Vec<(String, Vec<EvalRecordObservation>)>) -> Arc<Self> {
        Arc::new(Self {
            replies: Mutex::new(replies.into_iter().collect()),
        })
    }

    async fn pop(&self) -> (String, Vec<EvalRecordObservation>) {
        self.replies
            .lock()
            .await
            .pop_front()
            .expect("scripted agent has no remaining reply")
    }
}

fn reply(
    response: &str,
    records: Vec<EvalRecordObservation>,
) -> (String, Vec<EvalRecordObservation>) {
    (response.to_owned(), records)
}

struct TestOutcome {
    scenarios: Vec<ScenarioExecutionResults>,
    run: EvalResults,
}

async fn drive(
    tasks: Vec<EvalTask>,
    judge_outputs: Vec<Result<serde_json::Value, JudgeError>>,
    input_scenario: EvalScenario,
    replies: Vec<(String, Vec<EvalRecordObservation>)>,
) -> TestOutcome {
    let scoring = ScenarioScoring::with_in_memory_traces(
        Arc::new(spec(tasks)),
        MockJudgeInvoker::new(judge_outputs),
    )
    .expect("scoring builds");
    let agent = ScriptedAgent::new(replies);
    let mut state = RunState::open(eval_ref(), SimulatedUserMode::Client, vec![input_scenario]);
    let mut scenario_results: Vec<ScenarioExecutionResults> = Vec::new();
    let mut aggregation_inputs: Vec<ScenarioAggregationInput> = Vec::new();

    loop {
        let NextDirective(directive) = state.next().expect("next directive");
        match directive {
            TurnDirective::AgentTurn {
                scenario_id, turn, ..
            } => {
                let (response, records) = agent.pop().await;
                state
                    .submit_agent_turn(AgentTurnSubmission {
                        scenario_id,
                        turn,
                        response,
                        records,
                    })
                    .expect("submit agent turn");
            }
            TurnDirective::UserTurnNeeded { .. } => {
                panic!("no simulated user configured");
            }
            TurnDirective::ScenarioComplete { scenario_id } => {
                let cursor = state
                    .take_completed_scenario(&scenario_id)
                    .expect("take cursor");
                let result = scoring
                    .score_scenario(&cursor)
                    .await
                    .expect("score scenario");
                aggregation_inputs.push(
                    scoring
                        .scenario_aggregation(&cursor, &result)
                        .expect("scenario aggregation"),
                );
                scenario_results.push(result);
                state.ack_scenario_complete();
            }
            TurnDirective::RunComplete => {
                state.ack_run_complete();
                break;
            }
        }
    }

    let identity = RunIdentity {
        run_id: state.run_id,
        eval_ref: eval_ref(),
        started_at: state.opened_at,
        ended_at: Utc::now(),
    };
    let run = scoring
        .finalize(identity, aggregation_inputs)
        .expect("finalize");
    TestOutcome {
        scenarios: scenario_results,
        run,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn scripted_walk_llm_judge_parity() {
    let fixture = fixture_value(include_str!("fixtures/parity/scripted_judge.json"));
    assert_eq!(fixture["expected_pass_rate"], json!(1.0));
    let run_id = wyrd_spec::vala::ids::RunId::from_string("scripted-parity-run".to_owned());

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
    let fixture = fixture_value(include_str!("fixtures/parity/termination_signal.json"));
    assert_eq!(fixture["expected_scenario_passed"], json!(true));
    let run_id = wyrd_spec::vala::ids::RunId::from_string("signal-parity-run".to_owned());

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
    let fixture = fixture_value(include_str!("fixtures/parity/subject_rollup.json"));
    assert_eq!(fixture["expected_subject_pass_rate"], json!(1.0));
    let run_id = wyrd_spec::vala::ids::RunId::from_string("subject-parity-run".to_owned());

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
