//! Python boundary for the Vala eval protocol client.

#![cfg(feature = "python")]

use std::sync::Arc;
use std::time::Duration;

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyList, PyModule, PyString};
use url::Url;
use wyrd_semver::VersionBlock;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::protocol::{ConversationTurn, SimulatedUserMode, TurnRole};

use crate::eval::protocol_client::{
    AgentFn, AgentTurnOutput, ProtocolClient, ProtocolClientError, RunSummary, SimulatedUserFn,
};

/// Run an Eval card through the server-hosted pull protocol.
///
/// # Errors
/// Returns a structured Wyrd Python error when inputs, transport, protocol
/// decoding, or Python callbacks fail.
#[pyfunction(name = "run_eval")]
#[pyo3(signature = (
    server_url,
    eval_ref,
    agent_fn,
    *,
    access_token,
    simulated_user_fn = None,
    simulated_user = "server",
    request_timeout_secs = 60,
))]
pub fn run_eval_py(
    py: Python<'_>,
    server_url: &str,
    eval_ref: &str,
    agent_fn: Py<PyAny>,
    access_token: String,
    simulated_user_fn: Option<Py<PyAny>>,
    simulated_user: &str,
    request_timeout_secs: u64,
) -> PyResult<Py<PyAny>> {
    let url = Url::parse(server_url).map_err(|source| {
        wyrd_error_to_py(WyrdError::Validation {
            message: format!("invalid server_url: {source}"),
            details: serde_json::json!({ "field": "server_url", "value": server_url }),
        })
    })?;
    let eval_ref = parse_eval_ref(eval_ref).map_err(wyrd_error_to_py)?;
    let mode = parse_simulated_user_mode(simulated_user).map_err(wyrd_error_to_py)?;
    if matches!(mode, SimulatedUserMode::Client) && simulated_user_fn.is_none() {
        return Err(wyrd_error_to_py(WyrdError::Validation {
            message: "simulated_user='client' requires simulated_user_fn".to_string(),
            details: serde_json::json!({ "field": "simulated_user_fn" }),
        }));
    }

    let agent_obj = Arc::new(agent_fn);
    let simulated_obj = simulated_user_fn.map(Arc::new);
    let agent_closure: AgentFn = Box::new({
        let agent_obj = Arc::clone(&agent_obj);
        move |message, history| invoke_agent(&agent_obj, message, history)
    });
    let simulated_closure: Option<SimulatedUserFn> = if let Some(simulated_obj) = simulated_obj {
        let callback: SimulatedUserFn = Box::new(move |history: &[ConversationTurn]| {
            invoke_simulated_user(&simulated_obj, history)
        });
        Some(callback)
    } else {
        None
    };

    let client = ProtocolClient::new(url, Duration::from_secs(request_timeout_secs), access_token)
        .map_err(protocol_error_to_py)?;

    let summary = py
        .detach(|| client.run_eval(eval_ref, mode, agent_closure, simulated_closure))
        .map_err(protocol_error_to_py)?;
    summary_to_py(py, &summary)
}

/// Register the `wyrd._wyrd.eval` submodule.
///
/// # Errors
/// Returns PyO3 registration errors.
pub fn python_register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(run_eval_py, module)?)?;
    Ok(())
}

/// GIL re-entry point for agent turn callbacks — attaches to the interpreter, calls the Python callable, and extracts the result.
fn invoke_agent(
    agent_obj: &Py<PyAny>,
    message: &str,
    history: &[ConversationTurn],
) -> Result<AgentTurnOutput, ProtocolClientError> {
    Python::attach(|py| {
        let history_list =
            history_to_pylist(py, history).map_err(|error| callback_error("history", error))?;
        let result = agent_obj
            .call1(py, (message.to_owned(), history_list))
            .map_err(|error| ProtocolClientError::AgentFn {
                message: py_err_message(py, error),
            })?;
        agent_output_from_py(py, result)
    })
}

/// GIL re-entry point for simulated-user turn callbacks — mirrors [`invoke_agent`] for the user-side callback.
fn invoke_simulated_user(
    simulated_obj: &Py<PyAny>,
    history: &[ConversationTurn],
) -> Result<String, ProtocolClientError> {
    Python::attach(|py| {
        let history_list =
            history_to_pylist(py, history).map_err(|error| callback_error("history", error))?;
        simulated_obj
            .call1(py, (history_list,))
            .and_then(|result| result.extract::<String>(py))
            .map_err(|error| ProtocolClientError::SimulatedUserFn {
                message: py_err_message(py, error),
            })
    })
}

fn history_to_pylist(py: Python<'_>, history: &[ConversationTurn]) -> PyResult<Py<PyAny>> {
    let list = PyList::empty(py);
    for turn in history {
        let item = PyDict::new(py);
        item.set_item(
            "role",
            match turn.role {
                TurnRole::User => "user",
                TurnRole::Agent => "agent",
            },
        )?;
        item.set_item("content", &turn.content)?;
        list.append(item)?;
    }
    Ok(list.into_any().unbind())
}

fn agent_output_from_py(
    py: Python<'_>,
    obj: Py<PyAny>,
) -> Result<AgentTurnOutput, ProtocolClientError> {
    if let Ok(response) = obj.extract::<String>(py) {
        return Ok(AgentTurnOutput {
            response,
            records: Vec::new(),
        });
    }

    let bound = obj.bind(py);
    let response: String = bound
        .get_item("response")
        .and_then(|value| value.extract())
        .map_err(|error| ProtocolClientError::AgentFn {
            message: format!("agent_fn return value requires string response: {error}"),
        })?;
    let records = match bound.get_item("records") {
        Ok(value) if !value.is_none() => {
            let json = wyrd_utils::py::pyobject_to_json(&value).map_err(|error| {
                ProtocolClientError::AgentFn {
                    message: format!("records must be JSON-serializable: {error}"),
                }
            })?;
            serde_json::from_value(json).map_err(|error| ProtocolClientError::AgentFn {
                message: format!("records must be EvalRecordObservation values: {error}"),
            })?
        }
        Ok(_) | Err(_) => Vec::new(),
    };
    Ok(AgentTurnOutput { response, records })
}

fn summary_to_py(py: Python<'_>, summary: &RunSummary) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    dict.set_item("run_id", PyString::new(py, summary.run_id.as_str()))?;
    dict.set_item("server_url", PyString::new(py, &summary.server_url))?;
    Ok(dict.into_any().unbind())
}

fn parse_simulated_user_mode(value: &str) -> Result<SimulatedUserMode, WyrdError> {
    match value {
        "server" => Ok(SimulatedUserMode::Server),
        "client" => Ok(SimulatedUserMode::Client),
        other => Err(WyrdError::Validation {
            message: format!("simulated_user must be 'server' or 'client', got {other:?}"),
            details: serde_json::json!({
                "field": "simulated_user",
                "value": other,
                "allowed": ["server", "client"],
            }),
        }),
    }
}

fn parse_eval_ref(value: &str) -> Result<CardRef, WyrdError> {
    let (identity, version) = value
        .rsplit_once('@')
        .ok_or_else(|| WyrdError::Validation {
            message: "eval_ref must use space/name@version syntax".to_string(),
            details: serde_json::json!({ "field": "eval_ref", "value": value }),
        })?;
    let (space, name) = identity
        .split_once('/')
        .ok_or_else(|| WyrdError::Validation {
            message: "eval_ref must use space/name@version syntax".to_string(),
            details: serde_json::json!({ "field": "eval_ref", "value": value }),
        })?;
    Ok(CardRef {
        kind: CardKind::Eval,
        name: CardName::new(name).map_err(|source| invalid_ref_field("name", name, source))?,
        version: VersionBlock::parse(version)
            .map_err(|source| invalid_ref_field("version", version, source))?,
        space: parse_space(space)?,
        uid: None,
    })
}

fn parse_space(value: &str) -> Result<SpaceName, WyrdError> {
    SpaceName::new(value).map_err(|source| invalid_ref_field("space", value, source))
}

fn invalid_ref_field(
    field: &'static str,
    value: &str,
    source: impl std::fmt::Display,
) -> WyrdError {
    WyrdError::Validation {
        message: format!("invalid eval_ref {field}: {value}"),
        details: serde_json::json!({
            "field": format!("eval_ref.{field}"),
            "value": value,
            "source": source.to_string(),
        }),
    }
}

fn protocol_error_to_py(error: ProtocolClientError) -> PyErr {
    wyrd_error_to_py(match error {
        ProtocolClientError::HttpBuild { message } => WyrdError::Internal {
            message: "failed to build eval protocol HTTP client".to_string(),
            details: serde_json::json!({ "source": message }),
        },
        ProtocolClientError::Http {
            operation,
            message,
            status,
        } => WyrdError::UpstreamFailure {
            message: format!("eval protocol {operation} request failed"),
            details: serde_json::json!({
                "operation": operation,
                "status": status,
                "source": message,
            }),
        },
        ProtocolClientError::Url { operation, message } => WyrdError::Validation {
            message: format!("invalid eval protocol URL for {operation}"),
            details: serde_json::json!({ "operation": operation, "source": message }),
        },
        ProtocolClientError::Malformed { operation, message } => WyrdError::UpstreamFailure {
            message: format!("eval protocol {operation} response did not match the schema"),
            details: serde_json::json!({ "operation": operation, "source": message }),
        },
        ProtocolClientError::AgentFn { message } => WyrdError::AgentCallbackAborted {
            message,
            details: serde_json::json!({ "callback": "agent_fn" }),
        },
        ProtocolClientError::SimulatedUserFn { message } => WyrdError::AgentCallbackAborted {
            message,
            details: serde_json::json!({ "callback": "simulated_user_fn" }),
        },
    })
}

fn callback_error(context: &'static str, error: PyErr) -> ProtocolClientError {
    Python::attach(|py| ProtocolClientError::AgentFn {
        message: format!("{context}: {}", py_err_message(py, error)),
    })
}

fn py_err_message(py: Python<'_>, error: PyErr) -> String {
    error.value(py).to_string()
}

fn wyrd_error_to_py(error: WyrdError) -> PyErr {
    wyrd_utils::py::wyrd_error_to_py_err(error)
}
