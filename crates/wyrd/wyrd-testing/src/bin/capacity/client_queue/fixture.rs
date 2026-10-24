//! The benchmark tenant, provisioned the way a tenant administrator and an
//! application would through the public client.
//!
//! The administrator registers a Service whose one component is a Model
//! bound to the Window Verifier that routes its Drift samples, creates the
//! Service family's runtime account holding `workload`, and hydrates the
//! bundle the application's `WyrdState` loads offline. The account's key,
//! run as the registered Service root, emits Drift observations; the
//! administrator reads them back through Bifrost.

use std::path::{Path, PathBuf};

use secrecy::{ExposeSecret as _, SecretString};
use serde::Deserialize;
use wyrd_client::cards::{BundleMode, CardGraphPuller, CardSelector, Cards};
use wyrd_client::config::ClientConfig;
use wyrd_client::{Bifrost, GlobalConfig, Principals, WyrdClient};
use wyrd_spec::auth::{
    CreateServicePrincipalRequest, WorkloadFamily, WorkloadKind, WorkloadRestriction,
};
use wyrd_testing::release_server::{SERVER_URL, SetupTenant};

use crate::client_queue::Result;
use crate::client_queue::step::FEATURES;

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
        "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: {DRIFT_VERIFIER}\n  version: 1.0.0\nspec:\n  inputs:\n    samples: {{type: samples, features: [{names}]}}\n  requirements:\n    - id: f0-bounded\n      statement: f0 stays bounded.\n      check: {{kind: drift, on: inputs.samples, method: Custom, signal: {{kind: Metric, name: f0}}, condition: {{kind: Above, limit: 1000.0}}}}\n"
    )
}

/// The Model Card whose Drift observations the benchmark emits.
const MODEL: &str = "apiVersion: wyrd/v1\nkind: Model\nmetadata:\n  name: ingest-model\n  version: 1.0.0\nspec:\n  interface:\n    kind: Custom\n    meta:\n      framework_version: 0.1.0\n      loader_module: fixture\n      loader_class: TinyModel\n      extra: {}\n  task_type: Other\n  signature:\n    inputs:\n      - name: f0\n        dtype: float64\n    outputs:\n      - name: score\n        dtype: float64\n  card_refs: []\n";

/// The Service Card composing [`MODEL`] under the alias `model`, bound
/// without a schedule to the Window Verifier of `ingest.yaml`.
const SERVICE: &str = "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: ingest-service\n  version: 1.0.0\nspec:\n  components:\n    - alias: model\n      ref: ./model.yaml\n      verified_by:\n        - verifier: ./ingest.yaml\n";

/// One provisioned tenant and the identities its traffic uses.
pub struct Tenant {
    /// Tenant administrator credential: reads the durable rows back.
    admin_key: SecretString,
    /// The Service family runtime account's API key the emitting
    /// application presents; its `WyrdState` runs as the bundle's root.
    service_key: SecretString,
    /// Hydrated Service bundle every emitting `WyrdState` loads offline.
    pub bundle: PathBuf,
}

/// One `COUNT(*)`-shaped answer.
#[derive(Deserialize)]
struct Count {
    /// The counted rows.
    n: i64,
}

impl Tenant {
    /// Registers the Model and Service for `setup`'s tenant under `root`,
    /// creates the Service family's runtime account, and hydrates its bundle.
    ///
    /// # Errors
    ///
    /// Returns a file write, client, registration, key issuance, or hydration
    /// failure.
    pub async fn provision(setup: &SetupTenant, root: &Path) -> Result<Self> {
        std::fs::create_dir_all(root)?;
        std::fs::write(root.join("model.yaml"), MODEL)?;
        std::fs::write(root.join("ingest.yaml"), drift_verifier_yaml(FEATURES))?;
        std::fs::write(root.join("service.yaml"), SERVICE)?;
        let admin = connect(&setup.api_key, None)?;
        let cards = Cards::connect_with(&admin).await?;
        let service = Box::pin(cards.declare_from_path(&root.join("service.yaml"))).await?;
        let issued = Principals::connect_with(&admin)
            .await?
            .create_service_principal(&CreateServicePrincipalRequest {
                name: "bifrost-client-queue".to_owned(),
                roles: vec!["workload".to_owned()],
                description: None,
                workload_restriction: Some(WorkloadRestriction::Family(WorkloadFamily {
                    kind: WorkloadKind::Service,
                    name: service.root.name.clone(),
                })),
            })
            .await?;
        let bundle = root.join("bundle");
        Box::pin(CardGraphPuller::new(cards.registry_context()).pull(
            &CardSelector::exact(service.root),
            &bundle,
            BundleMode::Complete,
        ))
        .await?;
        Ok(Self {
            admin_key: SecretString::from(setup.api_key.expose_secret().to_owned()),
            service_key: SecretString::from(issued.credential.expose().to_owned()),
            bundle,
        })
    }

    /// A Service-authenticated client of the server, whose Bifrost gRPC
    /// dials `grpc` when given (the delay proxy) and the server's own gRPC
    /// port otherwise.
    ///
    /// # Errors
    ///
    /// Returns the client configuration failure.
    pub fn service(&self, grpc: Option<&str>) -> Result<WyrdClient> {
        connect(&self.service_key, grpc)
    }

    /// Counts the rows `sql` selects as `n` through the administrator's
    /// tenant-scoped Bifrost query.
    ///
    /// # Errors
    ///
    /// Returns the query failure or an answer without exactly one row.
    pub async fn count(&self, sql: &str) -> Result<u64> {
        let rows: Vec<Count> = Bifrost::query_only(&connect(&self.admin_key, None)?)
            .sql_as(sql, &[])
            .await?;
        match rows.as_slice() {
            [row] => Ok(u64::try_from(row.n)?),
            _ => Err(format!("`{sql}` returned {} rows", rows.len()).into()),
        }
    }
}

/// A public client of [`SERVER_URL`] presenting `credential`, with its gRPC
/// endpoint overridden to `grpc` when given.
///
/// # Errors
///
/// Returns the client configuration failure.
fn connect(credential: &SecretString, grpc: Option<&str>) -> Result<WyrdClient> {
    Ok(WyrdClient::with_config(ClientConfig {
        credential: Some(SecretString::from(credential.expose_secret().to_owned())),
        ..ClientConfig::from_global_with_overrides(&GlobalConfig::default(), Some(SERVER_URL), grpc)
    })?)
}
