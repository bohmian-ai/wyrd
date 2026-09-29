//! Score-only flow for `wyrd eval run --records`.

use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use chrono::Utc;
use wyrd_spec::vala::eval::record::EvalRecordObservation;
use wyrd_spec::vala::ids::RunId;

use crate::error::WyrdCliError;
use crate::eval::output;
use crate::eval::run::{self, EvalRunArgs};

/// Run `--records`.
///
/// # Errors
/// Returns an error when input loading, scoring, comparison, or output writing
/// fails.
pub async fn run_local_records(args: EvalRunArgs) -> Result<ExitCode, WyrdCliError> {
    let (eval_ref, spec) = run::load_eval_card(&args.eval)?;
    let records = load_records_jsonl(&args.records)?;
    let judge = run::judge_for_spec(&spec, args.judge_mock)?;
    let scoring =
        vala_eval::orchestrator::ScenarioScoring::with_in_memory_traces(Arc::new(spec), judge)
            .map_err(|source| WyrdCliError::Orchestrator { source })?;
    let now = Utc::now();
    let run = scoring
        .score_record_batch(
            vala_eval::RunIdentity {
                run_id: RunId::new(),
                eval_ref,
                started_at: now,
                ended_at: now,
            },
            Some(&args.subject),
            &records,
        )
        .await
        .map_err(|source| WyrdCliError::Orchestrator { source })?;
    let comparison = output::load_comparison(args.baseline.as_deref(), &run)?;
    let out_dir = output::write_results(args.out.as_ref(), &run, comparison.as_ref())?;
    output::print_local_summary(&run, comparison.as_ref(), Some(&out_dir));

    if args.fail_on_gate && !output::pass_gate_satisfied(&run) {
        return Ok(ExitCode::from(2));
    }
    Ok(ExitCode::SUCCESS)
}

fn load_records_jsonl(path: &Path) -> Result<Vec<EvalRecordObservation>, WyrdCliError> {
    let raw = std::fs::read_to_string(path).map_err(|source| WyrdCliError::Io { source })?;
    let mut records = Vec::new();
    for (index, line) in raw.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let record =
            serde_json::from_str(trimmed).map_err(|source| WyrdCliError::RecordsParse {
                path: path.display().to_string(),
                detail: format!("line {}: {source}", index + 1),
            })?;
        records.push(record);
    }
    Ok(records)
}
