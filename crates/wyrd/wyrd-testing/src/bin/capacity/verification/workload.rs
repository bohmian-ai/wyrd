//! The benchmark tenant's Card graph and the arrivals the steps send.
//!
//! The tenant registers, as a tenant administrator would, an assertion
//! Verifier of four checks over one 2 KiB `context` slot, a Model, and a
//! Service binding the Verifier to the Model, once per closed-loop client.
//! Each client is its own application: it loads its pulled Service bundle
//! with its Service family runtime account's key, run as that Service root,
//! so every `verify` and `submit` carries its own authenticated owner, exactly as `WyrdState` does in production
//! when many applications share a tenant.
//!
//! A run that selects a model step also registers a judge Verifier whose one
//! Agent check calls a local Gateway deployment for one bounded iteration,
//! and binds it beside the assertion Verifier; the deployment answers from a
//! local fixture provider, so no external model or credential is involved.

use std::path::Path;

use secrecy::{ExposeSecret as _, SecretString};
use wyrd_client::cards::{BundleMode, CardGraphPuller, CardSelector, Cards};
use wyrd_client::config::ClientConfig;
use wyrd_client::state::WyrdState;
use wyrd_client::{Gateway, GlobalConfig, Principals, WyrdClient};
use wyrd_spec::auth::{
    CreateServicePrincipalRequest, WorkloadFamily, WorkloadKind, WorkloadRestriction,
};
use wyrd_testing::release_server::{SERVER_URL, SetupTenant};

use crate::verification::Result;

/// The assertion Verifier's `metadata.name`, which every arrival names.
pub const ASSERT_VERIFIER: &str = "bench-assert";

/// The judge Verifier's `metadata.name`, which every model arrival names.
pub const JUDGE_VERIFIER: &str = "bench-judge";

/// The judge Verifier's one input slot, which its Verdict cites.
pub const JUDGE_SLOT: &str = "answer";

/// The local Gateway deployment the judge's Agent check routes through.
pub const JUDGE_DEPLOYMENT: &str = "bench-judge";

/// Bytes of padding that bring an assertion context to 2 KiB.
const PADDING: usize = 1_980;

/// The assertion Verifier: four `equals "yes"` checks over one `json` slot.
const ASSERT: &str = "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: bench-assert\n  version: 1.0.0\nspec:\n  inputs:\n    context: {type: json}\n  requirements:\n    - id: a\n      statement: a is yes.\n      check: {kind: assert, path: inputs.context.a, operator: equals, expected: \"yes\"}\n    - id: b\n      statement: b is yes.\n      check: {kind: assert, path: inputs.context.b, operator: equals, expected: \"yes\"}\n    - id: c\n      statement: c is yes.\n      check: {kind: assert, path: inputs.context.c, operator: equals, expected: \"yes\"}\n    - id: d\n      statement: d is yes.\n      check: {kind: assert, path: inputs.context.d, operator: equals, expected: \"yes\"}\n";

/// The judge Verifier: one Agent check over one `json` slot, routed through
/// [`JUDGE_DEPLOYMENT`] for one iteration, answering a plain cited Verdict.
const JUDGE: &str = "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: bench-judge\n  version: 1.0.0\nspec:\n  inputs:\n    answer: {type: json}\n  requirements:\n    - id: judged\n      statement: The answer is acceptable.\n      check:\n        kind: agent\n        inputs: {answer: inputs.answer}\n        agent:\n          execution_route: {kind: wyrd_gateway, deployment: bench-judge}\n          run_config: {max_iterations: 1}\n          prompt:\n            provider: openai\n            model: gpt-4o\n            messages:\n              - {role: user, content: \"Judge {{answer}}\"}\n            response_format: verdict\n";

/// The Model the Verifier judges.
const MODEL: &str = "apiVersion: wyrd/v1\nkind: Model\nmetadata:\n  name: bench-model\n  version: 1.0.0\nspec:\n  interface:\n    kind: Custom\n    meta:\n      framework_version: 0.1.0\n      loader_module: fixture\n      loader_class: TinyModel\n      extra: {}\n  task_type: Other\n  signature:\n    inputs:\n      - name: f0\n        dtype: float64\n    outputs:\n      - name: score\n        dtype: float64\n  card_refs: []\n";

/// The Service binding the assertion Verifier to the Model; each
/// application renames it `bench-service-<index>`.
const SERVICE: &str = "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: bench-service\n  version: 1.0.0\nspec:\n  components:\n    - alias: model\n      ref: ./model.yaml\n      verified_by:\n        - verifier: ./assert.yaml\n";

/// The judge's binding, appended to [`SERVICE`] when the run has model steps.
const JUDGE_BINDING: &str = "        - verifier: ./judge.yaml\n";

/// The Service every application renames: [`SERVICE`], binding the judge
/// too when `judge` is set.
fn service(judge: bool) -> String {
    let mut service = SERVICE.to_owned();
    if judge {
        service.push_str(JUDGE_BINDING);
    }
    service
}

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

/// One provisioned tenant: its applications' states.
pub struct Tenant {
    /// One application per closed-loop client: its pulled Service bundle
    /// under that Service's own Card-bound key.
    pub states: Vec<WyrdState>,
}

impl Tenant {
    /// Declares the Card graph for `setup`'s tenant under `root` with
    /// `applications` Services, then provisions each application through
    /// [`Self::application`]. With a `judge` provider base URL it first
    /// points [`JUDGE_DEPLOYMENT`] at that provider and binds the judge
    /// Verifier in every Service too.
    ///
    /// # Errors
    ///
    /// Returns a file, client, deployment, declaration, key, or pull failure.
    pub async fn provision(
        setup: &SetupTenant,
        root: &Path,
        applications: usize,
        judge: Option<&str>,
    ) -> Result<Self> {
        let directory = root.join(&setup.slug);
        std::fs::create_dir_all(&directory)?;
        for (name, body) in [("assert.yaml", ASSERT), ("model.yaml", MODEL)] {
            std::fs::write(directory.join(name), body)?;
        }
        let admin = connect(&setup.api_key)?;
        if let Some(provider) = judge {
            let deployment = serde_json::from_value(serde_json::json!({
                "name": JUDGE_DEPLOYMENT,
                "adapter": {"openai_compatible": {"base_url": format!("{provider}/v1")}},
                "auth": "none",
                "capabilities": ["chat_completions"],
            }))?;
            Gateway::connect_with(&admin)
                .await?
                .put_deployment(&deployment)
                .await?;
            std::fs::write(directory.join("judge.yaml"), JUDGE)?;
        }
        let service = service(judge.is_some());
        let cards = Cards::connect_with(&admin).await?;
        let mut states = Vec::with_capacity(applications);
        for index in 0..applications {
            states.push(Self::application(&admin, &cards, &directory, &service, index).await?);
        }
        Ok(Self { states })
    }

    /// Declares Service `bench-service-<index>` from `service` beside the
    /// shared Verifiers and Model in `directory` (an identical redeclaration
    /// of those reuses them), creates its family runtime account holding
    /// `workload`, pulls its bundle, and loads it; the state runs as the
    /// Service root.
    ///
    /// # Errors
    ///
    /// Returns a file, client, declaration, key, or pull failure.
    async fn application(
        admin: &WyrdClient,
        cards: &Cards,
        directory: &Path,
        service: &str,
        index: usize,
    ) -> Result<WyrdState> {
        let file = directory.join(format!("service-{index}.yaml"));
        let name = format!("name: bench-service-{index}\n");
        std::fs::write(&file, service.replacen("name: bench-service\n", &name, 1))?;
        let service = Box::pin(cards.declare_from_path(&file)).await?;
        let issued = Principals::connect_with(admin)
            .await?
            .create_service_principal(&CreateServicePrincipalRequest {
                name: format!("bench-service-{index}-runtime"),
                roles: vec!["workload".to_owned()],
                description: None,
                workload_restriction: Some(WorkloadRestriction::Family(WorkloadFamily {
                    kind: WorkloadKind::Service,
                    name: service.root.name.clone(),
                })),
            })
            .await
            .map_err(|error| format!("creating the Service account: {error}"))?;
        let bundle = directory.join(format!("bundle-{index}"));
        Box::pin(CardGraphPuller::new(cards.registry_context()).pull(
            &CardSelector::exact(service.root.clone()),
            &bundle,
            BundleMode::Complete,
        ))
        .await
        .map_err(|error| format!("pulling the Service bundle: {error}"))?;
        let key = SecretString::from(issued.credential.expose().to_owned());
        Ok(WyrdState::from_path_with(&bundle, connect(&key)?)?)
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
    use serde_json::{Value, json};

    use super::{ASSERT, JUDGE, JUDGE_DEPLOYMENT, JUDGE_SLOT, JUDGE_VERIFIER, failing, service};

    /// The JSON form of the Card YAML `card`.
    ///
    /// # Panics
    ///
    /// Panics when `card` is not a valid Card.
    fn card(card: &str) -> Value {
        serde_json::to_value(wyrd_spec::format::yaml::from_str(card).expect("a valid Card"))
            .expect("json")
    }

    /// The judge Verifier is one Agent check over its one slot, routed
    /// through the local deployment for one iteration and answering a plain
    /// Verdict; only a model run binds it beside the unchanged assertion
    /// Verifier.
    ///
    /// # Panics
    ///
    /// Panics when the judge or its binding drifts.
    #[test]
    fn model_fixture_declares_one_gateway_agent_check() {
        let judge = card(JUDGE);
        assert_eq!(judge["metadata"]["name"], JUDGE_VERIFIER);
        assert_eq!(
            judge["spec"]["inputs"]
                .as_object()
                .map(|inputs| inputs.keys().collect::<Vec<_>>()),
            Some(vec![&JUDGE_SLOT.to_owned()])
        );
        let requirements = judge["spec"]["requirements"]
            .as_array()
            .expect("requirements");
        assert_eq!(requirements.len(), 1);
        let check = &requirements[0]["check"];
        assert_eq!(check["kind"], "agent");
        assert_eq!(check["inputs"], json!({ JUDGE_SLOT: "inputs.answer" }));
        let agent = &check["agent"];
        assert_eq!(
            agent["execution_route"],
            json!({ "kind": "wyrd_gateway", "deployment": JUDGE_DEPLOYMENT })
        );
        assert_eq!(agent["run_config"]["max_iterations"], 1);
        assert_eq!(agent["prompt"]["response_format"], "verdict");
        assert_eq!(
            card(ASSERT)["spec"]["requirements"]
                .as_array()
                .map(Vec::len),
            Some(4)
        );
        let verifiers = |judge: bool| {
            card(&service(judge))["spec"]["components"][0]["verified_by"]
                .as_array()
                .expect("bindings")
                .iter()
                .map(|binding| binding["verifier"].as_str().unwrap_or_default().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(verifiers(false), ["./assert.yaml"]);
        assert_eq!(verifiers(true), ["./assert.yaml", "./judge.yaml"]);
    }

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
