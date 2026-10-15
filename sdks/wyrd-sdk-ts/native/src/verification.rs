//! Thin napi projection of Verifier inputs, submissions, and execution
//! handles.
//!
//! Slot projection, file pinning, routing, retry keys, and polling stay in
//! `wyrd_client::observe`; this module only converts Node values into the
//! shared [`Inputs`] and carries handles and catalog errors back.

use std::time::Duration;

use napi::Env;
use napi::bindgen_prelude::{Buffer, PromiseRaw};
use napi_derive::napi;
use serde_json::{Value, json};
use wyrd_client::observe::{ArtifactRef, File, Input, Inputs, Submission, VerificationExecution};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::ExecutionId;

use crate::cards::parse_card_ref;
use crate::client::NativeWyrdClient;
use crate::{NativeLifecycleResult, NativeWyrdError};

/// A local file for a `media` slot, read only when the call pins it.
#[napi(object)]
pub struct NativeFileInput {
    /// A file on disk; exclusive with `bytes`.
    pub path: Option<String>,
    /// In-memory contents; exclusive with `path`.
    pub bytes: Option<Buffer>,
    /// Canonical format name, such as `pdf`; inferred when omitted.
    pub format: Option<String>,
    /// Display name sent with the declaration.
    pub filename: Option<String>,
}

/// One named slot value: exactly one of `value_json`, `file`, or `artifact`.
#[napi(object)]
pub struct NativeVerifierInput {
    /// The Verifier input slot this value fills.
    pub slot: String,
    /// A JSON value for a `json`, `trace`, or `samples` slot.
    pub value_json: Option<String>,
    /// A local file for a `media` slot.
    pub file: Option<NativeFileInput>,
    /// An exact Artifact Card reference, as text or JSON, for a `media` slot.
    pub artifact: Option<String>,
    /// The file inside `artifact`, when it holds several.
    pub artifact_path: Option<String>,
}

/// Convert Node slot values into the shared client's named [`Inputs`].
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` for a slot value that is not exactly
/// one kind, malformed JSON, or a malformed Artifact reference, and
/// `WYRD_VERIFICATION_400_INPUT_INVALID` for an unknown file format or a file
/// that is not exactly one of path and bytes.
pub(crate) fn inputs(values: Vec<NativeVerifierInput>) -> Result<Inputs, WyrdError> {
    let mut named = Vec::with_capacity(values.len());
    for value in values {
        let input = match (value.value_json, value.file, value.artifact) {
            (Some(json), None, None) => {
                Input::Value(serde_json::from_str::<Value>(&json).map_err(|error| {
                    WyrdError::Validation {
                        message: format!("input {} is not JSON: {error}", value.slot),
                        details: json!({ "field": "inputs", "slot": value.slot }),
                    }
                })?)
            }
            (None, Some(file), None) => Input::File(file_input(&value.slot, file)?),
            (None, None, Some(artifact)) => Input::Artifact(ArtifactRef::new(
                parse_card_ref(&artifact)?,
                value.artifact_path,
            )),
            _ => {
                return Err(WyrdError::Validation {
                    message: format!(
                        "input {} must be exactly one value, File, or ArtifactRef",
                        value.slot
                    ),
                    details: json!({ "field": "inputs", "slot": value.slot }),
                });
            }
        };
        named.push((value.slot, input));
    }
    Ok(Inputs::named(named))
}

/// Build one lazy shared [`File`] from its Node description.
///
/// # Errors
/// Returns `WYRD_VERIFICATION_400_INPUT_INVALID` for an unknown format or a
/// file that is not exactly one of path and bytes.
fn file_input(slot: &str, input: NativeFileInput) -> Result<File, WyrdError> {
    let invalid = |message: String| WyrdError::VerificationInputInvalid {
        message,
        details: json!({ "slot": slot }),
    };
    let mut file = match (input.path, input.bytes) {
        (Some(path), None) => File::path(path),
        (None, Some(bytes)) => File::bytes(bytes.to_vec()),
        _ => {
            return Err(invalid(
                "a File is exactly one of a path or bytes".to_owned(),
            ));
        }
    };
    if let Some(format) = input.format {
        file = file.with_format(
            serde_json::from_value(Value::String(format.clone()))
                .map_err(|_| invalid(format!("unknown file format {format:?}")))?,
        );
    }
    if let Some(filename) = input.filename {
        file = file.with_filename(filename);
    }
    Ok(file)
}

/// Closed result of `submit` or `connect`: Recorded when both fields are
/// absent, a queued execution handle, or a catalog error.
#[napi(object, object_from_js = false)]
pub struct NativeSubmission {
    /// The queued execution, for Run evidence.
    pub execution: Option<NativeVerificationExecution>,
    /// Catalog failure otherwise.
    pub error: Option<NativeWyrdError>,
}

impl NativeSubmission {
    /// Project one shared submission outcome onto the closed result.
    pub(crate) fn outcome(result: Result<Submission, WyrdError>) -> Self {
        match result {
            Ok(Submission::Recorded) => Self {
                execution: None,
                error: None,
            },
            Ok(Submission::Queued(execution)) => Self::queued(execution),
            Err(error) => Self {
                execution: None,
                error: Some(NativeWyrdError::from_wyrd(&error)),
            },
        }
    }

    /// The result carrying one execution handle.
    fn queued(execution: VerificationExecution) -> Self {
        Self {
            execution: Some(NativeVerificationExecution { execution }),
            error: None,
        }
    }
}

/// Node-facing handle of one queued Verifier execution.
#[napi]
pub struct NativeVerificationExecution {
    /// The shared handle owning the ID and client.
    execution: VerificationExecution,
}

#[napi]
impl NativeVerificationExecution {
    /// The server-issued execution ID.
    #[napi(getter)]
    pub fn id(&self) -> String {
        self.execution.id().to_string()
    }

    /// Reads the execution's status, terminal error, and visible result.
    ///
    /// # Errors
    /// Returns a napi error only when the state cannot be projected; read
    /// failures are returned in [`NativeLifecycleResult`].
    #[napi]
    pub fn status<'env>(
        &self,
        env: &'env Env,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let execution = self.execution.clone();
        crate::trace::spawn(env, async move {
            NativeLifecycleResult::outcome(execution.status().await)
        })
    }

    /// Polls until a result, the execution's terminal error, or `timeout_ms`.
    ///
    /// # Errors
    /// Returns a napi error only when the result cannot be projected; the
    /// terminal error and `WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT` are returned
    /// in [`NativeLifecycleResult`].
    #[napi]
    pub fn wait<'env>(
        &self,
        env: &'env Env,
        timeout_ms: u32,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let execution = self.execution.clone();
        crate::trace::spawn(env, async move {
            NativeLifecycleResult::outcome(
                execution
                    .wait(Duration::from_millis(u64::from(timeout_ms)))
                    .await,
            )
        })
    }
}

#[napi]
impl NativeWyrdClient {
    /// Rebuilds the handle of execution `id` as this client's principal.
    ///
    /// # Errors
    /// Never returns a napi error; a malformed ID and a refused credential
    /// exchange are carried in the result.
    #[napi]
    pub fn verification_execution<'env>(
        &self,
        env: &'env Env,
        id: String,
    ) -> napi::Result<PromiseRaw<'env, NativeSubmission>> {
        let client = self.client.clone();
        crate::trace::spawn(env, async move {
            let connected = match id.parse::<ExecutionId>() {
                Ok(id) => VerificationExecution::connect_with(id, &client).await,
                Err(error) => Err(WyrdError::Validation {
                    message: format!("execution ID is invalid: {error}"),
                    details: json!({ "field": "id" }),
                }),
            };
            Ok(match connected {
                Ok(execution) => NativeSubmission::queued(execution),
                Err(error) => NativeSubmission::outcome(Err(error)),
            })
        })
    }
}
