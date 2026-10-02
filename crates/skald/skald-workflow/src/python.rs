//! Python error boundary for Workflow failures.
//!
//! The `wyrd.agent.Workflow` class lives in the Python SDK; this module keeps
//! only the conversion the orphan rule places with [`WorkflowError`].

#![cfg(feature = "python")]

use wyrd_spec::error::WyrdError;
use wyrd_utils::py::WyrdPyError;

use crate::error::WorkflowError;

impl From<WorkflowError> for WyrdPyError {
    /// Widen a workflow failure into the shared Python boundary error.
    ///
    /// The catalog projection in [`crate::error`] owns every public metadata
    /// field, so `?` on a [`WorkflowError`] inside a `#[pymethods]` body raises
    /// the shared `wyrd.WyrdError` with its canonical code and status.
    fn from(error: WorkflowError) -> Self {
        Self::from(WyrdError::from(error))
    }
}
