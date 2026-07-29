//! All-or-nothing Python holder construction for offline `WyrdState` bundles.

use std::collections::BTreeMap;

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyMapping};
use wyrd_cards::{agent::PyAgentCard, data::DataCard, model::ModelCard, prompt::PromptCard};
use wyrd_interfaces::error::CardPyResult;
use wyrd_spec::envelope::CardKind;

use super::{PyCardEnvelope, WyrdState, parse_load_config, runtime_hydration_error};

/// Normalized Python loader configuration keyed by exact `CardRef` identity.
pub(super) struct PythonLoadConfig {
    /// Interface objects retained independently of borrowed Python lifetimes.
    pub(super) interface_by_ref: BTreeMap<String, Py<PyAny>>,
    /// JSON-compatible loader kwargs retained by exact `CardRef`.
    pub(super) kwargs_by_ref: BTreeMap<String, Py<PyAny>>,
    /// Caller-approved canonical artifact manifests keyed by exact CardRef.
    pub(super) trusted_artifact_hashes_by_ref: BTreeMap<String, String>,
}

/// Builds every Python holder for one already-validated offline state bundle.
///
/// The hydrator owns the construction transaction: maps remain local until all
/// interface validation and eager local loads succeed, so callers can never
/// receive a partially hydrated `PyWyrdState`. It has no registry, cache, or
/// network dependency.
pub(super) struct PythonStateHydrator<'state> {
    /// Native graph that established exact references and artifact integrity.
    state: &'state WyrdState,
    /// Caller configuration normalized to exact persisted Card identities.
    config: PythonLoadConfig,
}

/// Fully built Python holder maps waiting to be published by `PyWyrdState`.
pub(super) struct HydratedPythonCards {
    /// Generic envelope projections keyed by exact CardRef.
    pub(super) envelopes: BTreeMap<String, Py<PyCardEnvelope>>,
    /// Eager Agent holders keyed by exact CardRef.
    pub(super) agents: BTreeMap<String, Py<PyAgentCard>>,
    /// Eager Prompt holders keyed by exact CardRef.
    pub(super) prompts: BTreeMap<String, Py<PromptCard>>,
    /// Eager Model holders keyed by exact CardRef.
    pub(super) models: BTreeMap<String, Py<ModelCard>>,
    /// Eager Data holders keyed by exact CardRef.
    pub(super) data: BTreeMap<String, Py<DataCard>>,
}

impl<'state> PythonStateHydrator<'state> {
    /// Create one all-or-nothing holder construction workflow.
    pub(super) fn new(state: &'state WyrdState, config: PythonLoadConfig) -> Self {
        Self { state, config }
    }

    /// Normalize Python mappings before any holder is created or artifact read.
    ///
    /// # Errors
    ///
    /// Returns stable alias, kind, mapping, or configuration errors without
    /// invoking an interface loader.
    pub(super) fn normalize_config(
        py: Python<'_>,
        state: &WyrdState,
        interfaces: Option<&Bound<'_, PyMapping>>,
        load_kwargs: Option<&Bound<'_, PyMapping>>,
        trusted_artifact_hashes: Option<&Bound<'_, PyMapping>>,
    ) -> CardPyResult<PythonLoadConfig> {
        parse_load_config(py, state, interfaces, load_kwargs, trusted_artifact_hashes)
    }

    /// Hydrate every public Python projection before publishing state.
    ///
    /// # Errors
    ///
    /// Returns the first stable construction-stage error. All maps remain
    /// local to this call, so an error publishes no partial state.
    pub(super) fn hydrate(self, py: Python<'_>) -> CardPyResult<HydratedPythonCards> {
        self.validate_executable_artifacts()?;
        Ok(HydratedPythonCards {
            envelopes: self.hydrate_envelopes(py)?,
            prompts: self.hydrate_prompts(py)?,
            agents: self.hydrate_agents(py)?,
            models: self.hydrate_models(py)?,
            data: self.hydrate_data(py)?,
        })
    }

    /// Require exact caller trust before a joblib-backed built-in Model loads.
    ///
    /// # Errors
    ///
    /// Returns a stable `artifact_trust` hydration error when trust is absent
    /// or differs from the canonical manifest hash validated from the bundle.
    fn validate_executable_artifacts(&self) -> CardPyResult<()> {
        for (key, card) in self.state.cards_of_kind(CardKind::Model) {
            let wyrd_spec::envelope::Spec::Model(spec) = &card.spec else {
                return Err(runtime_hydration_error(
                    self.state,
                    key,
                    "artifact_trust",
                    "model Card has an invalid spec",
                ));
            };
            if !matches!(
                spec.interface,
                wyrd_spec::card::model::ModelInterface::Sklearn(_)
                    | wyrd_spec::card::model::ModelInterface::Xgboost(_)
                    | wyrd_spec::card::model::ModelInterface::Lightgbm(_)
                    | wyrd_spec::card::model::ModelInterface::Catboost(_)
            ) {
                continue;
            }
            let actual = self.state.artifact_manifest_hash_by_key(key)?;
            let Some(actual) = actual else {
                return Err(runtime_hydration_error(
                    self.state,
                    key,
                    "artifact_trust",
                    "executable model has no artifact manifest",
                ));
            };
            match self.config.trusted_artifact_hashes_by_ref.get(key) {
                Some(expected) if expected == &actual => {}
                Some(_) => {
                    return Err(runtime_hydration_error(
                        self.state,
                        key,
                        "artifact_trust",
                        "trusted artifact manifest hash does not match",
                    ));
                }
                None => {
                    return Err(runtime_hydration_error(
                        self.state,
                        key,
                        "artifact_trust",
                        "executable model requires an exact trusted artifact manifest hash",
                    ));
                }
            }
        }
        Ok(())
    }
}
