//! The benchmark tenant's Card graph and the arrivals the steps send.
//!
//! The tenant registers, as a tenant administrator would, an assertion
//! Verifier of four checks over one 2 KiB `context` slot, a Model, and a
//! Service binding the Verifier to the Model. The application then loads the
//! hydrated Service bundle with a Card-bound Service key, so every `verify`
//! and `submit` carries an authenticated owner, exactly as `WyrdState` does
//! in production.

use std::path::Path;

use reqwest::Method;
use secrecy::{ExposeSecret as _, SecretString};
use wyrd_client::cards::{CardGraphHydrator, CardSelector, Cards, HydrationMode};
use wyrd_client::config::ClientConfig;
use wyrd_client::state::WyrdState;
use wyrd_client::{GlobalConfig, WyrdClient};
use wyrd_spec::auth::{IssueKeyRequest, IssueKeyResponse};
use wyrd_testing::release_server::{SERVER_URL, SetupTenant};

use crate::verification::Result;

/// The assertion Verifier's `metadata.name`, which every arrival names.
pub const ASSERT_VERIFIER: &str = "bench-assert";

/// Bytes of padding that bring an assertion context to 2 KiB.
const PADDING: usize = 1_980;

/// The assertion Verifier: four `equals "yes"` checks over one `json` slot.
const ASSERT: &str = "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: bench-assert\n  version: 1.0.0\n  space: default\nspec:\n  inputs:\n    context: {type: json}\n  requirements:\n    - id: a\n      statement: a is yes.\n      check: {kind: assert, path: inputs.context.a, operator: equals, expected: \"yes\"}\n    - id: b\n      statement: b is yes.\n      check: {kind: assert, path: inputs.context.b, operator: equals, expected: \"yes\"}\n    - id: c\n      statement: c is yes.\n      check: {kind: assert, path: inputs.context.c, operator: equals, expected: \"yes\"}\n    - id: d\n      statement: d is yes.\n      check: {kind: assert, path: inputs.context.d, operator: equals, expected: \"yes\"}\n";

/// The Model the Verifier judges.
const MODEL: &str = "apiVersion: wyrd/v1\nkind: Model\nmetadata:\n  name: bench-model\n  version: 1.0.0\n  space: default\nspec:\n  interface:\n    kind: Custom\n    meta:\n      framework_version: 0.1.0\n      loader_module: fixture\n      loader_class: TinyModel\n      extra: {}\n  task_type: Other\n  signature:\n    inputs:\n      - name: f0\n        dtype: float64\n    outputs:\n      - name: score\n        dtype: float64\n  card_refs: []\n";

/// The Service binding the assertion Verifier to the Model.
const SERVICE: &str = "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: bench-service\n  version: 1.0.0\n  space: default\nspec:\n  components:\n    - alias: model\n      ref: ./model.yaml\n      verified_by:\n        - verifier: ./assert.yaml\n";

/// Whether arrival `sequence` answers `d` wrong: every odd one, so both
/// verdicts stay in every window.
pub const fn failing(sequence: u64) -> bool {
    sequence % 2 == 1
}

/// The 2 KiB context of arrival `sequence`.
pub fn context(sequence: u64) -> serde_json::Value {
    let d = if failing(sequence) { "no" } else { "yes" };
    serde_json::json!({
        "a": "yes", "b": "yes", "c": "yes", "d": d,
        "padding": "x".repeat(PADDING),
    })
}

/// One provisioned tenant: the application's state.
pub struct Tenant {
    /// The application: the hydrated Service bundle under the Service key.
    pub state: WyrdState,
}

impl Tenant {
    /// Registers the Card graph for `setup`'s tenant under `root`, issues the
    /// Service its Card-bound key, hydrates the bundle, and loads it.
    ///
    /// # Errors
    ///
    /// Returns a file, client, registration, key, or hydration failure.
    pub async fn provision(setup: &SetupTenant, root: &Path) -> Result<Self> {
        let directory = root.join(&setup.slug);
        std::fs::create_dir_all(&directory)?;
        for (name, body) in [
            ("assert.yaml", ASSERT),
            ("model.yaml", MODEL),
            ("service.yaml", SERVICE),
        ] {
            std::fs::write(directory.join(name), body)?;
        }
        let admin = connect(&setup.api_key)?;
        let cards = Cards::connect_with(&admin).await?;
        let service = Box::pin(cards.register_from_path(&directory.join("service.yaml"))).await?;
        let issued: IssueKeyResponse = admin
            .request_json(
                Method::POST,
                "/auth/issue-key",
                Some(&IssueKeyRequest {
                    card_ref: service.root.clone(),
                    label: Some("bench".to_owned()),
                    expires_in_seconds: None,
                }),
            )
            .await
            .map_err(|error| format!("issuing the Service key: {error}"))?;
        let bundle = directory.join("bundle");
        Box::pin(CardGraphHydrator::new(cards.registry_context()).hydrate(
            &CardSelector::exact(service.root.clone()),
            &bundle,
            HydrationMode::Complete,
        ))
        .await
        .map_err(|error| format!("hydrating the Service bundle: {error}"))?;
        let key = SecretString::from(issued.key.expose().to_owned());
        Ok(Self {
            state: WyrdState::from_path_with(&bundle, connect(&key)?)?,
        })
    }
}

/// A public client of the server presenting `credential`.
///
/// # Errors
///
/// Returns the client configuration failure.
fn connect(credential: &SecretString) -> Result<WyrdClient> {
    Ok(WyrdClient::with_config(ClientConfig {
        credential: Some(SecretString::from(credential.expose_secret().to_owned())),
        ..ClientConfig::from_global_with_overrides(&GlobalConfig::default(), Some(SERVER_URL), None)
    })?)
}

#[cfg(test)]
mod tests {
    use super::failing;

    /// Every odd arrival fails and every even one passes.
    ///
    /// # Panics
    ///
    /// Panics when an arrival's failure is miscomputed.
    #[test]
    fn odd_arrivals_fail() {
        let failing: Vec<u64> = (0..6).filter(|s| failing(*s)).collect();
        assert_eq!(failing, vec![1, 3, 5]);
    }
}
