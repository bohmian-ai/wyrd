//! `wyrd.prompt` registration: the shared Skald Prompt classes, plus the
//! Verdict and code review factories this SDK adds to their `ResponseFormat`
//! and the decision-provider constructors it adds to `Prompt`.

use std::collections::BTreeMap;

use pyo3::prelude::*;
use pyo3::types::{PyMapping, PyModule};
use skald_prompt::{Prompt, PromptBuilderError, ResponseFormat};
use wyrd_client::verify::{DecisionQuestion, JevPromptOptions, JevQuestion};
use wyrd_utils::py::WyrdPyResult;

use crate::observe::{invalid_argument, json_value};

/// The canonical Verdict format, optionally requiring custom `data`
/// described by a Pydantic model class or a JSON Schema, normalized as a
/// Prompt's structured output is.
///
/// # Errors
/// Raises `WyrdError` when `data` is neither a model class nor a JSON Schema
/// object.
#[pyfunction]
#[pyo3(signature = (data = None))]
fn verdict(data: Option<&Bound<'_, PyAny>>) -> WyrdPyResult<ResponseFormat> {
    let Some(data) = data.filter(|data| !data.is_none()) else {
        return Ok(ResponseFormat::verdict(None));
    };
    let serde_json::Value::Object(schema) = skald_prompt::prompt::output_schema_from_py(data)?
    else {
        return Err(PromptBuilderError::InvalidResponseSchema.into());
    };
    Ok(ResponseFormat::verdict(Some(schema)))
}

/// A TypeSafe Jev `system_one` Prompt asking `questions` about `state`,
/// usually a `${name}` placeholder bound per call.
///
/// Each question is a `wyrd.prompt.decisions` model, lowered by the shared
/// Rust builder (Noul criteria to Jev's `{"true", "false"}`), or a native Jev
/// question mapping, kept as authored; one map may mix both.
///
/// # Errors
/// Raises `WyrdError` (`WYRD_SPEC_400_VALIDATION`) naming `questions.<key>`
/// for a question that is neither form, and the builder's error for a blank
/// model or a broken Jev bound.
#[pyfunction]
#[pyo3(signature = (model, *, state, questions))]
fn jev(model: String, state: String, questions: &Bound<'_, PyMapping>) -> WyrdPyResult<Prompt> {
    let py = questions.py();
    let questions = questions
        .items()?
        .iter()
        .map(|entry| {
            let (name, question): (String, Bound<'_, PyAny>) = entry.extract()?;
            let field = format!("questions.{name}");
            let value = json_value(py, &field, &question)?;
            let native = if question.hasattr("model_dump_json")? {
                serde_json::from_value::<DecisionQuestion>(value).map(JevQuestion::from)
            } else {
                serde_json::from_value::<JevQuestion>(value)
            }
            .map_err(|error| invalid_argument(&field, error))?;
            Ok((name, native))
        })
        .collect::<WyrdPyResult<BTreeMap<_, _>>>()?;
    Ok(skald_prompt::jev(
        model,
        JevPromptOptions { state, questions },
    )?)
}

/// The standard code review report format, `code_review`, whose fixed
/// schema GitHub delivery renders.
#[pyfunction]
fn code_review() -> ResponseFormat {
    ResponseFormat::code_review()
}

/// Register the shared Prompt classes on `module`, add `verdict` and
/// `code_review` to its `ResponseFormat` and `jev` to its `Prompt` as static
/// methods, so they and each class's other constructors share one class
/// identity.
///
/// # Errors
/// Propagates the Python error from registration or attribute assignment.
pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    skald_prompt::register_prompt(module)?;
    let py = module.py();
    let staticmethod = py.import("builtins")?.getattr("staticmethod")?;
    let format = module.getattr("ResponseFormat")?;
    format.setattr(
        "verdict",
        staticmethod.call1((wrap_pyfunction!(verdict, module)?,))?,
    )?;
    format.setattr(
        "code_review",
        staticmethod.call1((wrap_pyfunction!(code_review, module)?,))?,
    )?;
    module.getattr("Prompt")?.setattr(
        "jev",
        staticmethod.call1((wrap_pyfunction!(jev, module)?,))?,
    )
}
