//! One benchmark tenant, provisioned the way a tenant administrator and an
//! application would through the public client.
//!
//! The administrator registers a Custom Drift Verifier, a minutely schedule
//! Trigger, and a Service whose Model component is Drift-verified on that
//! Trigger and whose Agent component is Eval-verified on
//! `observations_ready` with a local HTTP Operator for failed verdicts; it
//! registers the custom dataset table, issues the Service's Card-bound key
//! through `POST /auth/issue-key`, and hydrates the Service bundle the
//! application loads offline.
//!
//! One step has no public surface: a Card-registered Service principal is
//! projected with no role, and no route grants one afterwards, so the
//! database owner grants the built-in `admin` role in one statement before
//! traffic starts. That is the only direct database write the benchmark
//! makes, and the report names it as a product gap.

use std::path::{Path, PathBuf};

use reqwest::Method;
use secrecy::{ExposeSecret as _, SecretString};
use serde::Deserialize;
use sqlx::PgPool;
use wyrd_client::bifrost::TableConfig;
use wyrd_client::cards::{CardGraphHydrator, CardSelector, Cards, HydrationMode};
use wyrd_client::config::ClientConfig;
use wyrd_client::state::WyrdState;
use wyrd_client::{Bifrost, GlobalConfig, WyrdClient};
use wyrd_spec::auth::{IssueKeyRequest, IssueKeyResponse};
use wyrd_spec::ids::CardUid;
use wyrd_testing::release_server::{SERVER_URL, SetupTenant};

use crate::Result;

/// Fully qualified custom dataset table every iteration writes one row into.
pub const TABLE: &str = "vala.datasets.verification_bench";

/// Grants the built-in `admin` role to the principal owning one API key.
///
/// The operator setup step standing in for the missing public role grant on
/// a Card-registered Service principal; see the module docs.
const GRANT_ADMIN_SQL: &str = "INSERT INTO wyrd.auth_service_account_roles \
       (data_tenant_id, service_account_id, role_id) \
     SELECT k.data_tenant_id, k.principal_id, r.id \
       FROM wyrd.auth_api_keys k \
       JOIN wyrd.auth_roles r ON r.data_tenant_id = k.data_tenant_id AND r.name = 'admin' \
      WHERE k.id = $1 \
     ON CONFLICT DO NOTHING";

/// One provisioned tenant and the identities its traffic and tally use.
pub struct Tenant {
    /// Tenant slug, also the Operator endpoint's path segment.
    pub slug: String,
    /// Data tenant id `setup` printed; never sent anywhere.
    pub tenant_id: String,
    /// Tenant administrator client: registry, manual Drift, and read-back.
    pub admin: WyrdClient,
    /// The Service's Card-bound API key the application clients present.
    pub service_key: SecretString,
    /// Hydrated Service bundle every application client loads offline.
    pub bundle: PathBuf,
    /// Exact Model component UID, the Drift subject.
    pub model_uid: CardUid,
    /// Exact Agent component UID, the Eval subject.
    pub agent_uid: CardUid,
    /// Exact Custom Drift Verifier UID, the manual run's Verifier.
    pub drift_verifier_uid: CardUid,
}

/// One `COUNT(*)`-shaped answer.
#[derive(Deserialize)]
struct Count {
    /// The counted rows.
    n: i64,
}

impl Tenant {
    /// Provisions `setup`'s tenant under `root`, with its Operator posting to
    /// `operator_url`, granting the Service key's role through `owner`.
    ///
    /// # Errors
    ///
    /// Returns a client, registration, hydration, key, or grant failure.
    pub async fn provision(
        setup: &SetupTenant,
        operator_url: &str,
        owner: &PgPool,
        root: &Path,
    ) -> Result<Self> {
        let directory = root.join(&setup.slug);
        std::fs::create_dir_all(&directory)?;
        write_graph(&directory, operator_url)?;
        let admin = connect(&setup.api_key)?;
        let cards = Cards::with_client(WyrdClient::clone(&admin));
        let verifier = Box::pin(cards.register_from_path(&directory.join("drift.yaml"))).await?;
        Box::pin(cards.register_from_path(&directory.join("trigger.yaml"))).await?;
        let service = Box::pin(cards.register_from_path(&directory.join("service.yaml"))).await?;

        let table = TableConfig::from_json_schema(
            TABLE,
            &serde_json::json!({
                "type": "object",
                "properties": { "sequence": { "type": "integer" } },
                "required": ["sequence"],
            }),
        )?;
        let writer = Bifrost::connect_with_table(&admin, table).await?;
        writer.register().await?;
        writer.shutdown().await?;

        let issued: IssueKeyResponse = admin
            .request_json(
                Method::POST,
                "/auth/issue-key",
                Some(&IssueKeyRequest {
                    card_ref: service.root.clone(),
                    label: Some("verification-bench".to_owned()),
                    expires_in_seconds: None,
                }),
            )
            .await?;
        sqlx::query(GRANT_ADMIN_SQL)
            .bind(issued.key_id)
            .execute(owner)
            .await?;

        let bundle = directory.join("bundle");
        Box::pin(CardGraphHydrator::new(cards.registry_context()).hydrate(
            &CardSelector::exact(service.root.clone()),
            &bundle,
            HydrationMode::Complete,
        ))
        .await?;
        let state = WyrdState::from_path(&bundle)?;
        let uid = |alias: &str| -> Result<CardUid> {
            Ok(state
                .run_for_card(alias)?
                .card_ref()
                .uid
                .clone()
                .ok_or("a hydrated Card carries its UID")?)
        };
        Ok(Self {
            slug: setup.slug.clone(),
            tenant_id: setup.tenant_id.clone(),
            service_key: SecretString::from(issued.key.expose().to_owned()),
            model_uid: uid("model")?,
            agent_uid: uid("agent")?,
            drift_verifier_uid: verifier
                .root
                .uid
                .ok_or("a registered Card carries its UID")?,
            admin,
            bundle,
        })
    }

    /// A fresh public client authenticated with the Service's Card-bound key.
    ///
    /// # Errors
    ///
    /// Returns the client configuration failure.
    pub fn service_client(&self) -> Result<WyrdClient> {
        connect(&self.service_key)
    }

    /// Counts the rows `sql` selects as `n`, read back through the tenant
    /// administrator's tenant-scoped Bifrost query.
    ///
    /// # Errors
    ///
    /// Returns the query failure or an answer without exactly one row.
    pub async fn count(&self, sql: &str) -> Result<u64> {
        let rows: Vec<Count> = Bifrost::query_only(&self.admin).sql_as(sql).await?;
        match rows.as_slice() {
            [row] => Ok(u64::try_from(row.n)?),
            _ => Err(format!("`{sql}` returned {} rows", rows.len()).into()),
        }
    }
}

/// A public client at the server's default URL presenting `credential`.
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

/// Writes the tenant's Card graph into `directory`.
///
/// # Errors
///
/// Returns the file write failure.
fn write_graph(directory: &Path, operator_url: &str) -> Result<()> {
    let files = [
        (
            "drift.yaml",
            "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: bench-drift\n  version: 1.0.0\n  space: default\nspec:\n  implementation:\n    kind: drift\n    spec:\n      method: Custom\n      signal:\n        kind: Metric\n        name: score\n      condition:\n        kind: Statistical\n      profile:\n        kind: Custom\n        metric_name: score\n        baseline_value: 1.0\n        alert_threshold: 0.5\n".to_owned(),
        ),
        (
            "trigger.yaml",
            "apiVersion: wyrd/v1\nkind: Trigger\nmetadata:\n  name: bench-minutely\n  version: 1.0.0\n  space: default\nspec:\n  kind: schedule\n  cron: \"* * * * *\"\n".to_owned(),
        ),
        (
            "model.yaml",
            "apiVersion: wyrd/v1\nkind: Model\nmetadata:\n  name: bench-model\n  version: 1.0.0\n  space: default\nspec:\n  interface:\n    kind: Custom\n    meta:\n      framework_version: 0.1.0\n      loader_module: fixture\n      loader_class: TinyModel\n      extra: {}\n  task_type: Other\n  signature:\n    inputs:\n      - name: latency\n        dtype: float64\n    outputs:\n      - name: score\n        dtype: float64\n  card_refs: []\n".to_owned(),
        ),
        (
            "agent-prompt.yaml",
            "apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  name: bench-agent-prompt\n  version: 1.0.0\n  space: default\nspec:\n  provider: openai\n  model: gpt-test\n  messages: [answer the question]\n".to_owned(),
        ),
        (
            "agent.yaml",
            "apiVersion: wyrd/v1\nkind: Agent\nmetadata:\n  name: bench-agent\n  version: 1.0.0\n  space: default\nspec:\n  prompt: ./agent-prompt.yaml\n  run_config:\n    max_iterations: 1\n".to_owned(),
        ),
        (
            "eval.yaml",
            "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: bench-eval\n  version: 1.0.0\n  space: default\nspec:\n  implementation:\n    kind: eval\n    spec:\n      pass_gate: {kind: all_pass}\n      tasks:\n        answer: {kind: assertion, id: answer, context_path: $.answer, operator: equals, expected: \"yes\"}\n".to_owned(),
        ),
        (
            "operator.yaml",
            format!(
                "apiVersion: wyrd/v1\nkind: Operator\nmetadata:\n  name: bench-operator\n  version: 1.0.0\n  space: default\nspec:\n  kind: http\n  method: post\n  url: {operator_url}\n"
            ),
        ),
        (
            "service.yaml",
            "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: bench-service\n  version: 1.0.0\n  space: default\nspec:\n  service_type: agent\n  components:\n    - alias: model\n      ref: ./model.yaml\n      verified_by:\n        - verifier: {kind: Verifier, name: bench-drift, version: 1.0.0, space: default}\n          runs_on: {kind: Trigger, name: bench-minutely, version: 1.0.0, space: default}\n    - alias: agent\n      ref: ./agent.yaml\n      verified_by:\n        - verifier: ./eval.yaml\n          runs_on: {kind: observations_ready}\n          on_failure: [./operator.yaml]\n".to_owned(),
        ),
    ];
    for (name, body) in files {
        std::fs::write(directory.join(name), body)?;
    }
    Ok(())
}
