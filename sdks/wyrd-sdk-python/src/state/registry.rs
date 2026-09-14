//! Python Card registration orchestration owned by an authenticated registry handle.

use pyo3::prelude::*;
use wyrd_client::cards::Cards;
use wyrd_interfaces::error::CardPyResult;
use wyrd_spec::envelope::CardKind;

use super::{PyRegistrationReceipt, register_python_card};

/// Owns the authenticated context for one Python registration operation.
pub(super) struct PythonCardRegistry<'a> {
    /// Shared authenticated Cards handle whose transport, storage, and tenant
    /// context every registration delegates to; borrowed so the Python view
    /// that owns it stays the single owner.
    registry: &'a Cards,
}

/// Delegates Python holders into the native registration workflow.
impl<'a> PythonCardRegistry<'a> {
    /// Borrow an authenticated registry context for registration calls.
    pub(super) fn new(registry: &'a Cards) -> Self {
        Self { registry }
    }

    /// Register one supported Python Card and return its server receipt.
    ///
    /// Python callback work runs before this method is called; the delegated
    /// native workflow performs hashing and network IO detached from the GIL.
    ///
    /// # Errors
    /// Returns validation, serialization, artifact, transport, or server
    /// completion errors from the native registration workflow.
    pub(super) fn register(
        &self,
        py: Python<'_>,
        card: &Bound<'_, PyAny>,
        version_bump: Option<&Bound<'_, PyAny>>,
        save_args: Option<&Bound<'_, PyAny>>,
    ) -> CardPyResult<PyRegistrationReceipt> {
        register_python_card(py, self.registry, card, version_bump, save_args, None)
    }

    /// Register a typed Card while enforcing its kind at the boundary.
    ///
    /// # Errors
    /// Returns the same registration errors as [`Self::register`], plus a
    /// stable kind-mismatch validation error when the holder is wrong.
    pub(super) fn register_typed(
        &self,
        py: Python<'_>,
        card: &Bound<'_, PyAny>,
        version_bump: Option<&Bound<'_, PyAny>>,
        save_args: Option<&Bound<'_, PyAny>>,
        kind: &CardKind,
    ) -> CardPyResult<PyRegistrationReceipt> {
        register_python_card(py, self.registry, card, version_bump, save_args, Some(kind))
    }
}
