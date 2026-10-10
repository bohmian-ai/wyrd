//! The `observe.verify` projection: caller input → one direct-execution request.
//!
//! A Run view judges its subject with a Verifier bound to it in the hydrated
//! graph. This module turns the caller's input into the wire input the
//! Verifier's kind takes, in the same forms `observe.eval` and
//! `observe.drift` accept, and owns the one transport call to
//! `POST /v1/verification/execute`. It writes no observation, run, or
//! dispatch; the server records the judgment as one result of the Run.

use std::collections::BTreeMap;
use std::time::Duration;

use reqwest::Method;
#[cfg(feature = "internal")]
use serde_json::value::RawValue;
use serde_json::{Value, json};
use wyrd_spec::card::verifier::VerifierImplementation;
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::eval::media::MediaRef;
use wyrd_spec::verification::{
    DirectVerificationInput, DriftSample, EXECUTION_DEADLINE, ExecuteVerificationRequest, Judgment,
};

use crate::client::WyrdClient;
use crate::observe::{drift, invalid_observation};

/// Time an execute response may take to arrive after the server's
/// [`EXECUTION_DEADLINE`] elapses: upload, audit, and the 504 answer itself.
const EXECUTE_RESPONSE_GRACE: Duration = Duration::from_secs(10);

/// Feature name to its samples in row order, the Drift wire input.
type DriftColumns = BTreeMap<String, Vec<Option<DriftSample>>>;

/// Build the wire input a Verifier of `implementation`'s kind judges.
///
/// An Eval Verifier takes one context object with optional media, and a task
/// Verifier takes the context its one check judges, also with optional media.
/// A Drift
/// Verifier takes a non-empty sequence of flat feature rows, each validated as
/// `observe.drift` validates one observation, and receives them as one column
/// per feature; a feature a row omits is a null sample in that row, which the
/// server judges. Media is refused for Drift because it has no meaning there.
///
/// # Errors
/// Returns `WYRD_SDK_400_INVALID_OBSERVATION` when the input's shape does not
/// match the Verifier's kind or a Drift row is not a flat object of supported
/// scalars.
pub(crate) fn direct_input(
    implementation: &VerifierImplementation,
    input: Value,
    media: Vec<MediaRef>,
) -> Result<DirectVerificationInput, WyrdError> {
    match implementation {
        VerifierImplementation::Eval(_) => {
            let Value::Object(context) = input else {
                return Err(invalid_observation(
                    "an Eval Verifier judges one context object",
                    json!({ "received": shape(&input) }),
                ));
            };
            Ok(DirectVerificationInput::EvalRecord {
                context,
                media: (!media.is_empty()).then_some(media),
            })
        }
        VerifierImplementation::Task(_) => {
            let Value::Object(context) = input else {
                return Err(invalid_observation(
                    "a task Verifier judges one context object",
                    json!({ "received": shape(&input) }),
                ));
            };
            Ok(DirectVerificationInput::TaskContext {
                context,
                media: (!media.is_empty()).then_some(media),
            })
        }
        VerifierImplementation::Drift(_) => {
            if !media.is_empty() {
                return Err(invalid_observation(
                    "a Drift Verifier takes no media",
                    json!({ "media": media.len() }),
                ));
            }
            Ok(DirectVerificationInput::DriftSamples {
                columns: drift_columns(&input)?,
            })
        }
    }
}

/// Refuse an integer literal beyond 64-bit range in any Drift row of JSON text.
///
/// The text form of [`direct_input`] parses rows into `f64` where such a
/// literal silently rounds; each row's raw text is checked first, exactly as
/// `observe.drift` checks its one object. Text that is not an array is left
/// to [`direct_input`], which reports its shape.
///
/// # Errors
/// Returns `WYRD_SDK_400_INVALID_OBSERVATION` naming the first such feature.
#[cfg(feature = "internal")]
pub(crate) fn check_drift_integer_literals(text: &str) -> Result<(), WyrdError> {
    let Ok(rows) = serde_json::from_str::<Vec<Box<RawValue>>>(text) else {
        return Ok(());
    };
    rows.iter()
        .try_for_each(|row| drift::check_integer_literals(row.get()))
}

/// Convert feature rows into the Drift wire columns.
///
/// # Errors
/// As [`direct_input`] for a Drift Verifier.
fn drift_columns(input: &Value) -> Result<DriftColumns, WyrdError> {
    let Value::Array(rows) = input else {
        return Err(invalid_observation(
            "a Drift Verifier judges a sequence of feature rows",
            json!({ "received": shape(input) }),
        ));
    };
    if rows.is_empty() {
        return Err(invalid_observation(
            "a Drift Verifier needs at least one feature row",
            Value::Null,
        ));
    }
    let mut columns = DriftColumns::new();
    for (index, row) in rows.iter().enumerate() {
        let record = drift::observation(row, None)?;
        for (feature, value) in &record.features {
            let sample = match drift::projected_value(feature, value)? {
                (Value::Number(number), _) => number.as_f64().map(DriftSample::Number),
                (_, Value::String(text)) => Some(DriftSample::Text(text)),
                _ => None,
            };
            columns
                .entry(feature.as_str().to_owned())
                .or_insert_with(|| vec![None; index])
                .push(sample);
        }
        for column in columns.values_mut() {
            column.resize(index + 1, None);
        }
    }
    Ok(columns)
}

/// Name a JSON value's shape for a structured refusal.
fn shape(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Send one direct execution and return its judgment.
///
/// The request carries no `Idempotency-Key`, so an ambiguous transport
/// failure is surfaced rather than replayed: a judge call is never silently
/// repeated. The client waits [`EXECUTION_DEADLINE`] plus
/// [`EXECUTE_RESPONSE_GRACE`] even when its configured timeout is shorter, so
/// a slow judgment ends in the server's
/// `WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT`.
///
/// # Errors
/// Returns `WYRD_VERIFICATION_413_INPUT_TOO_LARGE` locally when the input
/// exceeds a wire bound, and otherwise the server's refusal: malformed,
/// incompatible, or unsupported input, a caller without `verifier:run` on the
/// Verifier, an unknown Card, an unready or legacy baseline, a failing
/// judge provider, the deadline, or a transport failure.
///
/// # Cancellation
/// Dropping the future abandons the request; the server cancels in-flight
/// work on disconnect, though provider calls already issued may have run.
pub(crate) async fn execute(
    client: &WyrdClient,
    request: &ExecuteVerificationRequest,
) -> Result<Judgment, WyrdError> {
    request.validate()?;
    client
        .with_min_request_timeout(EXECUTION_DEADLINE + EXECUTE_RESPONSE_GRACE)
        .http
        .request_json(Method::POST, "/v1/verification/execute", Some(request))
        .await
}
