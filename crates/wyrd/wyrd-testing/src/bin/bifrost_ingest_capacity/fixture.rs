//! The benchmark tenant, provisioned the way a tenant administrator and an
//! application would through the public client.
//!
//! The administrator registers a Service whose one component is a Model,
//! issues the Service its Card-bound key, and hydrates the bundle the
//! application's `WyrdState` loads offline. Registration gives the Service
//! principal the `workload` role, so its key emits Drift observations; the
//! administrator reads them back through Bifrost.

use std::path::{Path, PathBuf};

use reqwest::Method;
use secrecy::{ExposeSecret as _, SecretString};
use serde::Deserialize;
use wyrd_client::cards::{CardGraphHydrator, CardSelector, Cards, HydrationMode};
use wyrd_client::config::ClientConfig;
use wyrd_client::{Bifrost, GlobalConfig, WyrdClient};
use wyrd_spec::auth::{IssueKeyRequest, IssueKeyResponse};
use wyrd_testing::release_server::{SERVER_URL, SetupTenant};

use crate::Result;

/// The Model Card whose Drift observations the benchmark emits.
const MODEL: &str = "apiVersion: wyrd/v1\nkind: Model\nmetadata:\n  name: ingest-model\n  version: 1.0.0\n  space: default\nspec:\n  interface:\n    kind: Custom\n    meta:\n      framework_version: 0.1.0\n      loader_module: fixture\n      loader_class: TinyModel\n      extra: {}\n  task_type: Other\n  signature:\n    inputs:\n      - name: f0\n        dtype: float64\n    outputs:\n      - name: score\n        dtype: float64\n  card_refs: []\n";

/// The Service Card composing [`MODEL`] under the alias `model`.
const SERVICE: &str = "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: ingest-service\n  version: 1.0.0\n  space: default\nspec:\n  components:\n    - alias: model\n      ref: ./model.yaml\n";

/// One provisioned tenant and the identities its traffic uses.
pub struct Tenant {
    /// Tenant administrator credential: reads the durable rows back.
    admin_key: SecretString,
    /// The Service's Card-bound API key the emitting application presents.
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
    /// issues the Service key, and hydrates its bundle.
    ///
    /// # Errors
    ///
    /// Returns a file write, client, registration, key issuance, or hydration
    /// failure.
    pub async fn provision(setup: &SetupTenant, root: &Path) -> Result<Self> {
        std::fs::create_dir_all(root)?;
        std::fs::write(root.join("model.yaml"), MODEL)?;
        std::fs::write(root.join("service.yaml"), SERVICE)?;
        let admin = connect(&setup.api_key, None)?;
        let cards = Cards::with_client(WyrdClient::clone(&admin));
        let service = Box::pin(cards.register_from_path(&root.join("service.yaml"))).await?;
        let issued: IssueKeyResponse = admin
            .request_json(
                Method::POST,
                "/auth/issue-key",
                Some(&IssueKeyRequest {
                    card_ref: service.root.clone(),
                    label: Some("bifrost-ingest-capacity".to_owned()),
                    expires_in_seconds: None,
                }),
            )
            .await?;
        let bundle = root.join("bundle");
        Box::pin(CardGraphHydrator::new(cards.registry_context()).hydrate(
            &CardSelector::exact(service.root),
            &bundle,
            HydrationMode::Complete,
        ))
        .await?;
        Ok(Self {
            admin_key: SecretString::from(setup.api_key.expose_secret().to_owned()),
            service_key: SecretString::from(issued.key.expose().to_owned()),
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
            .sql_as(sql)
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
