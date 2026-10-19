//! Benchmark tenants, provisioned the way a tenant administrator and an
//! application would through the public client, and the AC-040 reference
//! workload they verify.
//!
//! Each tenant registers an assertion Verifier of four checks over one 2 KiB
//! `context` slot, a Window Verifier whose `samples` slot declares every
//! ingested feature, and a Service whose Model component is bound to both
//! without a schedule: the assertion Verifier is executed and enqueued with
//! inline inputs, and the Window Verifier only routes recorded Drift samples,
//! so nothing activates either on its own.
//! Registration gives each Service principal the `workload` role, so its
//! Card-bound key emits; the administrator executes and enqueues Verifiers.

use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::Method;
use secrecy::{ExposeSecret as _, SecretString};
use wyrd_client::cards::{BundleMode, CardGraphPuller, CardSelector, Cards};
use wyrd_client::config::ClientConfig;
use wyrd_client::state::WyrdState;
use wyrd_client::{GlobalConfig, WyrdClient};
use wyrd_spec::auth::{IssueKeyRequest, IssueKeyResponse};
use wyrd_spec::ids::{CardUid, SlotName};
use wyrd_spec::verification::EXECUTION_DEADLINE;
use wyrd_spec::verification::execution::{InputDeclaration, VerificationRequest};
use wyrd_testing::release_server::{SERVER_URL, SetupTenant};

use crate::Result;
use crate::load::INGEST_FEATURES;

/// HTTP deadline of the direct-execution client: the server's
/// [`EXECUTION_DEADLINE`] plus time for the upload, audit, and 504 answer.
const EXECUTE_TIMEOUT: Duration = EXECUTION_DEADLINE.saturating_add(Duration::from_secs(10));

/// Bytes of padding that bring an assertion context to 2 KiB (AC-040).
const PADDING: usize = 1_980;

/// The assertion Verifier's one `json` slot.
const CONTEXT_SLOT: &str = "context";

/// The Window Verifier whose `samples` slot records the model's Drift
/// observations through `observe.submit`.
pub const DRIFT_VERIFIER: &str = "capacity-ingest";

/// The Card YAML of [`DRIFT_VERIFIER`]: one `samples` slot declaring
/// `f0` through `f{features - 1}` and one Custom drift requirement over `f0`,
/// so the client routes an observation of every declared feature to the
/// Drift observation path. It is bound without a schedule, so it never runs.
pub fn drift_verifier_yaml(features: usize) -> String {
    let names = (0..features)
        .map(|feature| format!("f{feature}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: {DRIFT_VERIFIER}\n  version: 1.0.0\n  space: default\nspec:\n  inputs:\n    samples: {{type: samples, features: [{names}]}}\n  requirements:\n    - id: f0-bounded\n      statement: f0 stays bounded.\n      check: {{kind: drift, on: inputs.samples, method: Custom, signal: {{kind: Metric, name: f0}}, condition: {{kind: Above, limit: 1000.0}}}}\n"
    )
}

/// The direct-execution or enqueue request of `verifier` judging `subject`
/// over the assertion [`context`], which fails its judgment when `failing`.
///
/// # Errors
///
/// Returns an error when [`CONTEXT_SLOT`] is not a valid slot name.
pub fn request(
    verifier: &CardUid,
    subject: &CardUid,
    failing: bool,
) -> Result<VerificationRequest> {
    Ok(VerificationRequest {
        verifier_card_uid: verifier.clone(),
        card_uid: subject.clone(),
        run_id: None,
        group_id: None,
        inputs: [(
            SlotName::new(CONTEXT_SLOT)?,
            InputDeclaration::Json {
                value: context(failing),
            },
        )]
        .into_iter()
        .collect(),
        execution_route_overrides: std::collections::BTreeMap::new(),
    })
}

/// The 2 KiB assertion context of four answered fields plus padding, its
/// last answer wrong when `failing`.
fn context(failing: bool) -> serde_json::Value {
    serde_json::json!({
        "a": "yes", "b": "yes", "c": "yes", "d": if failing { "no" } else { "yes" },
        "padding": "x".repeat(PADDING),
    })
}

/// One provisioned tenant and the identities its traffic uses.
pub struct Tenant {
    /// Tenant administrator credential: executes and enqueues Verifiers.
    pub admin_key: SecretString,
    /// The Service's Card-bound API key its application clients present.
    pub service_key: SecretString,
    /// Hydrated Service bundle every application client loads offline.
    pub bundle: PathBuf,
    /// Exact UID of the assertion Verifier.
    pub verifier: CardUid,
    /// Exact UID of the Model the assertion Verifier judges.
    pub subject: CardUid,
}

impl Tenant {
    /// Provisions `setup`'s tenant under `root` with every workload.
    ///
    /// # Errors
    ///
    /// Returns a client, registration, hydration, or key failure.
    ///
    /// # Cancellation
    ///
    /// Each registration and the issued key are durable as soon as their
    /// call returns, so a dropped provisioning leaves a partial tenant in the
    /// database and its files under `root`. No [`Tenant`] is returned and
    /// nothing measures it; registering again is idempotent per Card
    /// version, but a retry is only meaningful on the fresh database a new
    /// benchmark run gets.
    pub async fn provision(setup: &SetupTenant, root: &Path) -> Result<Self> {
        let directory = root.join(&setup.slug);
        std::fs::create_dir_all(&directory)?;
        write_graph(&directory)?;
        let admin = connect(SERVER_URL, &setup.api_key)?;
        let cards = Cards::connect_with(&admin).await?;
        let verifier = Box::pin(cards.declare_from_path(&directory.join("assert.yaml")))
            .await?
            .root;
        let verifier = uid(verifier.uid.as_ref())?;
        let service = Box::pin(cards.declare_from_path(&directory.join("service.yaml"))).await?;
        let issued: IssueKeyResponse = admin
            .request_json(
                Method::POST,
                "/auth/issue-key",
                Some(&IssueKeyRequest {
                    card_ref: service.root.clone(),
                    label: Some("capacity".to_owned()),
                    expires_in_seconds: None,
                }),
            )
            .await
            .map_err(|error| format!("issuing the Service key: {error}"))?;
        let bundle = directory.join("bundle");
        Box::pin(CardGraphPuller::new(cards.registry_context()).pull(
            &CardSelector::exact(service.root.clone()),
            &bundle,
            BundleMode::Complete,
        ))
        .await
        .map_err(|error| format!("hydrating the Service bundle: {error}"))?;
        let subject = uid(WyrdState::from_path(&bundle)?
            .run_for_card("model")?
            .subject()
            .uid
            .as_ref())?;
        Ok(Self {
            admin_key: SecretString::from(setup.api_key.expose_secret().to_owned()),
            service_key: SecretString::from(issued.key.expose().to_owned()),
            bundle,
            verifier,
            subject,
        })
    }

    /// An administrator client of the replica at `url`.
    ///
    /// # Errors
    ///
    /// Returns the client configuration failure.
    pub fn admin(&self, url: &str) -> Result<WyrdClient> {
        connect(url, &self.admin_key)
    }

    /// An administrator client of the replica at `url` for direct execution.
    ///
    /// Its HTTP deadline is [`EXECUTE_TIMEOUT`] rather than the default, so a
    /// judgment slower than the server's [`EXECUTION_DEADLINE`] ends in the
    /// server's own `WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT` instead of a
    /// client-side timeout.
    ///
    /// # Errors
    ///
    /// Returns the client configuration failure.
    pub fn executor(&self, url: &str) -> Result<WyrdClient> {
        let mut config = client_config(url, &self.admin_key);
        config.http.timeout_ms = u64::try_from(EXECUTE_TIMEOUT.as_millis())?;
        Ok(WyrdClient::with_config(config)?)
    }

    /// A Service-authenticated client of the replica at `url`.
    ///
    /// # Errors
    ///
    /// Returns the client configuration failure.
    pub fn service(&self, url: &str) -> Result<WyrdClient> {
        connect(url, &self.service_key)
    }
}

/// The UID a registered or hydrated Card carries.
///
/// # Errors
///
/// Returns an error when it carries none.
fn uid(uid: Option<&CardUid>) -> Result<CardUid> {
    uid.cloned()
        .ok_or_else(|| "a registered Card carries its UID".into())
}

/// A public client at `url` presenting `credential`.
///
/// # Errors
///
/// Returns the client configuration failure.
fn connect(url: &str, credential: &SecretString) -> Result<WyrdClient> {
    Ok(WyrdClient::with_config(client_config(url, credential))?)
}

/// The default public client configuration at `url` presenting `credential`.
fn client_config(url: &str, credential: &SecretString) -> ClientConfig {
    ClientConfig {
        credential: Some(SecretString::from(credential.expose_secret().to_owned())),
        ..ClientConfig::from_global_with_overrides(&GlobalConfig::default(), Some(url), None)
    }
}

/// Writes the tenant's Card graph into `directory`: the assertion Verifier,
/// [`DRIFT_VERIFIER`], the Model, and the Service binding both to the Model.
///
/// # Errors
///
/// Returns the file write failure.
fn write_graph(directory: &Path) -> Result<()> {
    let assertion = |id: &str| {
        format!(
            "    - id: {id}\n      statement: {id} is yes.\n      check: {{kind: assert, path: inputs.{CONTEXT_SLOT}.{id}, operator: equals, expected: \"yes\"}}\n"
        )
    };
    let files = [
        (
            "assert.yaml",
            format!(
                "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: capacity-assert\n  version: 1.0.0\n  space: default\nspec:\n  inputs:\n    {CONTEXT_SLOT}: {{type: json}}\n  requirements:\n{}",
                ["a", "b", "c", "d"].map(assertion).concat()
            ),
        ),
        ("ingest.yaml", drift_verifier_yaml(INGEST_FEATURES)),
        (
            "model.yaml",
            "apiVersion: wyrd/v1\nkind: Model\nmetadata:\n  name: capacity-model\n  version: 1.0.0\n  space: default\nspec:\n  interface:\n    kind: Custom\n    meta:\n      framework_version: 0.1.0\n      loader_module: fixture\n      loader_class: TinyModel\n      extra: {}\n  task_type: Other\n  signature:\n    inputs:\n      - name: f0\n        dtype: float64\n    outputs:\n      - name: score\n        dtype: float64\n  card_refs: []\n".to_owned(),
        ),
        (
            "service.yaml",
            "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: capacity-service\n  version: 1.0.0\n  space: default\nspec:\n  components:\n    - alias: model\n      ref: ./model.yaml\n      verified_by:\n        - verifier: ./assert.yaml\n        - verifier: ./ingest.yaml\n".to_owned(),
        ),
    ];
    for (name, body) in files {
        std::fs::write(directory.join(name), body)?;
    }
    Ok(())
}
