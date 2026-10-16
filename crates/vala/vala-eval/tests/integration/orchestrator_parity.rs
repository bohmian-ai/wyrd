use crate::orchestrator_support;

use std::collections::VecDeque;
use std::sync::Arc;

use chrono::Utc;
use orchestrator_support::{
    assertion_task, eval_ref, fixture_value, record, scenario, spec, subject_ref, tid,
};
use serde_json::json;
use tokio::sync::Mutex;
use vala_eval::orchestrator::{NextDirective, RunState, ScenarioScoring};
use vala_eval::{
    EvalResults, RunIdentity, ScenarioAggregationInput, ScenarioExecutionResults, SubjectKey,
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

/// Drive one scenario through `RunState` the way the hosted orchestrator does:
/// answer each agent turn from `replies` in order, score each completed
/// scenario through [`ScenarioScoring`], and finalize the run aggregate.
///
/// The loop ends at `RunComplete`; scoring uses in-memory traces, so no IO
/// leaves the process.
///
/// # Panics
/// Panics when scoring cannot be built from `tasks`, the state machine refuses
/// a directive or submission, the scripted agent runs out of replies, the
/// scenario requests a simulated user turn, or scoring and finalization fail.
async fn drive(
    tasks: Vec<EvalTask>,
    input_scenario: EvalScenario,
    replies: Vec<(String, Vec<EvalRecordObservation>)>,
) -> TestOutcome {
    let scoring =
        ScenarioScoring::with_in_memory_traces(Arc::new(spec(tasks))).expect("scoring builds");
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
                        .scenario_aggregation(&cursor, &result, Some(&subject_ref()))
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
        eval_ref: Some(eval_ref()),
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

/// A scripted multi-turn walk scores every turn's record and reaches the
/// fixture's expected pass rate.
///
/// # Panics
/// Panics when the fixture's expected pass rate is not 1.0, or when the walk
/// does not score both turns' records as passing at that rate.
#[tokio::test(flavor = "multi_thread")]
async fn scripted_walk_parity() {
    let fixture = fixture_value(include_str!("fixtures/parity/scripted_walk.json"));
    assert_eq!(fixture["expected_pass_rate"], json!(1.0));
    let outcome = drive(
        vec![assertion_task("context_ok")],
        scenario("scripted_walk", vec!["Continue"], None, 2),
        vec![
            reply("First answer", vec![record(true)]),
            reply("DONE", vec![record(true)]),
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

/// A termination signal ends every scenario with the fixture's expected outcome.
#[tokio::test(flavor = "multi_thread")]
async fn termination_signal_parity() {
    let fixture = fixture_value(include_str!("fixtures/parity/termination_signal.json"));
    assert_eq!(fixture["expected_scenario_passed"], json!(true));
    let outcome = drive(
        vec![assertion_task("context_ok")],
        scenario("termination_signal", Vec::new(), Some("DONE"), 8),
        vec![reply("DONE", vec![record(true)])],
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

/// Per-subject outcomes roll up exactly as the fixture expects.
#[tokio::test(flavor = "multi_thread")]
async fn subject_aggregation_parity() {
    let fixture = fixture_value(include_str!("fixtures/parity/subject_rollup.json"));
    assert_eq!(fixture["expected_subject_pass_rate"], json!(1.0));
    let outcome = drive(
        vec![assertion_task("subject_context_ok")],
        scenario("subject_rollup", Vec::new(), Some("DONE"), 1),
        vec![reply("DONE", vec![record(true)])],
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
