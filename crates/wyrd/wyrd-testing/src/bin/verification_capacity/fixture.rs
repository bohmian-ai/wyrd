//! Benchmark tenants, provisioned the way a tenant administrator and an
//! application would through the public client, and the AC-040 reference
//! workloads they verify.
//!
//! A measured tenant registers a Parquet baseline Data Card of
//! [`BASELINE_ROWS`] rows, PSI, SPC, and Custom Drift Verifiers, an
//! assertion Eval and an LLM-judge Eval Verifier, and a Service whose Model
//! component emits Drift samples and whose two Agent components are each
//! Eval-verified on `observations_ready`. It waits for both baselines to fit
//! and seeds one window of [`SAMPLES`] Drift observations that every queued
//! Drift run reads. A background tenant registers only the assertion Agent.
//! Registration gives each Service principal the `workload` role, so its
//! Card-bound key emits; the administrator runs and executes Verifiers.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use arrow::array::{ArrayRef, Float64Array, RecordBatch};
use arrow::datatypes::{DataType, Field, Schema};
use base64::Engine as _;
use chrono::{DateTime, Utc};
use reqwest::Method;
use secrecy::{ExposeSecret as _, SecretString};
use serde::Deserialize;
use sha2::Digest as _;
use wyrd_client::cards::{CardGraphHydrator, CardSelector, Cards, HydrationMode};
use wyrd_client::config::ClientConfig;
use wyrd_client::state::WyrdState;
use wyrd_client::{Bifrost, GlobalConfig, QueueConfig, WyrdClient};
use wyrd_spec::auth::{IssueKeyRequest, IssueKeyResponse};
use wyrd_spec::ids::CardUid;
use wyrd_spec::reference::CardRef;
use wyrd_spec::verification::{DirectVerificationInput, DriftSample};
use wyrd_testing::release_server::{SERVER_URL, SetupTenant};

use crate::Result;

/// Rows in the fitted baseline (AC-040).
const BASELINE_ROWS: u32 = 10_000;

/// Samples per feature in one Drift input (AC-040).
pub const SAMPLES: u32 = 1_000;

/// Numeric features of the baseline; PSI reads all of them (AC-040).
const FEATURES: usize = 8;

/// Drift rows one seeded observation lands: one per feature plus `score`.
const ROWS_PER_SAMPLE: u64 = FEATURES as u64 + 1;

/// Features SPC reads (AC-040).
const SPC_FEATURES: usize = 4;

/// Bytes of padding that bring an assertion context to 2 KiB (AC-040).
const PADDING: usize = 1_980;

/// How long a baseline fit or the seeded window may take to settle.
const SETTLE: Duration = Duration::from_secs(300);

/// The five measured verifier workloads, in report order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// PSI Drift over [`FEATURES`] features.
    Psi,
    /// SPC Drift over [`SPC_FEATURES`] features.
    Spc,
    /// Custom Drift over one metric.
    Custom,
    /// Eval of four assertions.
    Assertion,
    /// Eval of one LLM judge plus one assertion.
    Judge,
}

impl Kind {
    /// Every workload, in report order.
    pub const ALL: [Self; 5] = [
        Self::Psi,
        Self::Spc,
        Self::Custom,
        Self::Assertion,
        Self::Judge,
    ];

    /// The server's closed `kind` metric label for this workload.
    pub fn label(self) -> &'static str {
        match self {
            Self::Psi => "drift_psi",
            Self::Spc => "drift_spc",
            Self::Custom => "drift_custom",
            Self::Assertion => "eval_assertion",
            Self::Judge => "eval_llm_judge",
        }
    }

    /// Whether this workload is Drift, judged over samples.
    pub fn is_drift(self) -> bool {
        matches!(self, Self::Psi | Self::Spc | Self::Custom)
    }

    /// The Service component alias whose observations trigger a queued Eval.
    pub fn agent_alias(self) -> &'static str {
        match self {
            Self::Judge => "judge",
            _ => "assert",
        }
    }

    /// The direct-execution input for `sequence`, which fails its judgment
    /// when `failing`: an Eval context with a wrong answer, or Drift samples
    /// shifted far from the baseline.
    pub fn input(self, sequence: u64, failing: bool) -> DirectVerificationInput {
        let shift = if failing { 1_000.0 } else { 0.0 };
        let samples = |feature: usize| -> Vec<Option<DriftSample>> {
            (0..SAMPLES)
                .map(|row| {
                    Some(DriftSample::Number(
                        value(row + sequence as u32, feature) + shift,
                    ))
                })
                .collect()
        };
        match self {
            Self::Psi | Self::Spc => {
                let features = if self == Self::Psi {
                    FEATURES
                } else {
                    SPC_FEATURES
                };
                DirectVerificationInput::DriftSamples {
                    columns: (0..features)
                        .map(|feature| (format!("f{feature}"), samples(feature)))
                        .collect(),
                }
            }
            Self::Custom => DirectVerificationInput::DriftSamples {
                columns: [(
                    "score".to_owned(),
                    (0..SAMPLES)
                        .map(|_| Some(DriftSample::Number(if failing { 3.0 } else { 1.2 })))
                        .collect(),
                )]
                .into_iter()
                .collect(),
            },
            Self::Assertion | Self::Judge => DirectVerificationInput::EvalRecord {
                context: context(self, failing),
                media: None,
            },
        }
    }
}

/// The Eval context of `kind`, wrong when `failing`. The assertion context is
/// 2 KiB of four answered fields plus padding.
pub fn context(kind: Kind, failing: bool) -> serde_json::Map<String, serde_json::Value> {
    let answer = if failing { "no" } else { "yes" };
    let value = if kind == Kind::Judge {
        serde_json::json!({ "answer": answer })
    } else {
        serde_json::json!({
            "a": "yes", "b": "yes", "c": "yes", "d": answer,
            "padding": "x".repeat(PADDING),
        })
    };
    match value {
        serde_json::Value::Object(map) => map,
        _ => serde_json::Map::new(),
    }
}

/// Baseline value of `feature` at `row`: a permutation spreading every
/// feature evenly over `[0, 100)`, so a later sample window matches it.
fn value(row: u32, feature: usize) -> f64 {
    let index = (u64::from(row) * 7_919 + feature as u64 * 104_729) % u64::from(BASELINE_ROWS);
    index as f64 / 100.0
}

/// One provisioned tenant and the identities its traffic uses.
pub struct Tenant {
    /// Tenant slug.
    pub slug: String,
    /// Data tenant id `setup` printed; never sent anywhere.
    pub tenant_id: String,
    /// Tenant administrator credential: runs, executes, and reads back.
    pub admin_key: SecretString,
    /// The Service's Card-bound API key its application clients present.
    pub service_key: SecretString,
    /// Hydrated Service bundle every application client loads offline.
    pub bundle: PathBuf,
    /// Exact Verifier UID and subject UID of each provisioned workload.
    pub targets: Vec<(Kind, CardUid, CardUid)>,
    /// The seeded Drift window `[start, end)` every queued Drift run reads.
    pub window: Option<(DateTime<Utc>, DateTime<Utc>)>,
}

/// One `COUNT(*)`-shaped answer.
#[derive(Deserialize)]
struct Count {
    /// The counted rows.
    n: i64,
}

impl Tenant {
    /// Provisions `setup`'s tenant under `root`: every workload when
    /// `measured`, otherwise only the assertion Agent.
    ///
    /// # Errors
    ///
    /// Returns a client, registration, fit, hydration, key, or seeding
    /// failure.
    pub async fn provision(setup: &SetupTenant, root: &Path, measured: bool) -> Result<Self> {
        let directory = root.join(&setup.slug);
        std::fs::create_dir_all(&directory)?;
        write_graph(&directory, measured)?;
        let admin = connect(SERVER_URL, &setup.api_key)?;
        let cards = Cards::with_client(WyrdClient::clone(&admin));
        let register = |name: &str| {
            let path = directory.join(name);
            let cards = &cards;
            async move { Box::pin(cards.register_from_path(&path)).await }
        };
        let mut verifiers = Vec::new();
        let mut fits = Vec::new();
        if measured {
            register("baseline.yaml").await?;
            for (kind, name) in [
                (Kind::Psi, "psi.yaml"),
                (Kind::Spc, "spc.yaml"),
                (Kind::Custom, "custom.yaml"),
                (Kind::Judge, "judge.yaml"),
            ] {
                let root = register(name).await?.root;
                verifiers.push((kind, uid(&root.uid)?));
                if matches!(kind, Kind::Psi | Kind::Spc) {
                    fits.push((kind, root));
                }
            }
        }
        verifiers.push((
            Kind::Assertion,
            uid(&register("assert.yaml").await?.root.uid)?,
        ));
        let service = register("service.yaml").await?;

        let issued: IssueKeyResponse = admin
            .request_json(
                Method::POST,
                "/auth/issue-key",
                Some(&IssueKeyRequest {
                    card_ref: service.root.clone(),
                    label: Some("verification-capacity".to_owned()),
                    expires_in_seconds: None,
                }),
            )
            .await?;
        let bundle = directory.join("bundle");
        Box::pin(CardGraphHydrator::new(cards.registry_context()).hydrate(
            &CardSelector::exact(service.root.clone()),
            &bundle,
            HydrationMode::Complete,
        ))
        .await?;
        let state = WyrdState::from_path(&bundle)?;
        let component =
            |alias: &str| -> Result<CardUid> { uid(&state.run_for_card(alias)?.card_ref().uid) };
        let model_uid = if measured {
            Some(component("model")?)
        } else {
            None
        };
        let mut targets = Vec::new();
        for (kind, verifier) in verifiers {
            let subject = if kind.is_drift() {
                model_uid.clone().ok_or("a Drift workload has a Model")?
            } else {
                component(kind.agent_alias())?
            };
            targets.push((kind, verifier, subject));
        }
        let mut tenant = Self {
            slug: setup.slug.clone(),
            tenant_id: setup.tenant_id.clone(),
            admin_key: SecretString::from(setup.api_key.expose_secret().to_owned()),
            service_key: SecretString::from(issued.key.expose().to_owned()),
            bundle,
            targets,
            window: None,
        };
        if measured {
            for (kind, root) in fits {
                await_fit(&cards, kind, root).await?;
            }
            tenant.window = Some(tenant.seed().await?);
        }
        Ok(tenant)
    }

    /// The exact Verifier and subject UIDs of `kind`.
    ///
    /// # Errors
    ///
    /// Returns an error when this tenant does not run `kind`.
    pub fn target(&self, kind: Kind) -> Result<(&CardUid, &CardUid)> {
        self.targets
            .iter()
            .find(|(candidate, _, _)| *candidate == kind)
            .map(|(_, verifier, subject)| (verifier, subject))
            .ok_or_else(|| format!("{} does not run {kind:?}", self.slug).into())
    }

    /// An administrator client of the replica at `url`.
    ///
    /// # Errors
    ///
    /// Returns the client configuration failure.
    pub fn admin(&self, url: &str) -> Result<WyrdClient> {
        connect(url, &self.admin_key)
    }

    /// A Service-authenticated client of the replica at `url`.
    ///
    /// # Errors
    ///
    /// Returns the client configuration failure.
    pub fn service(&self, url: &str) -> Result<WyrdClient> {
        connect(url, &self.service_key)
    }

    /// Counts the rows `sql` selects as `n` through the administrator's
    /// tenant-scoped Bifrost query.
    ///
    /// # Errors
    ///
    /// Returns the query failure or an answer without exactly one row.
    pub async fn count(&self, sql: &str) -> Result<u64> {
        let rows: Vec<Count> = Bifrost::query_only(&self.admin(SERVER_URL)?)
            .sql_as(sql)
            .await?;
        match rows.as_slice() {
            [row] => Ok(u64::try_from(row.n)?),
            _ => Err(format!("`{sql}` returned {} rows", rows.len()).into()),
        }
    }

    /// Emits [`SAMPLES`] Drift observations of the Model through one Service
    /// lifetime, waits until all of their rows are queryable, and returns the
    /// window that holds exactly them.
    ///
    /// Each observation lands one row per feature plus its `score`. Admission
    /// is all-or-none per observation, so one refused with `QUEUE_FULL`
    /// admitted no row and is resubmitted unchanged once a flush frees the
    /// budget.
    ///
    /// # Errors
    ///
    /// Returns an emit failure other than `QUEUE_FULL`, a flush, drain, or
    /// query failure, or a window that never settles within [`SETTLE`].
    async fn seed(&self) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
        let start = Utc::now();
        let state = WyrdState::from_path(&self.bundle)?;
        state
            .start_bifrost_with_config(&self.service(SERVER_URL)?, None, QueueConfig::default())
            .await?;
        let run = state.run();
        let model = run.for_card("model")?;
        for row in 0..SAMPLES {
            let mut features = serde_json::Map::new();
            for feature in 0..FEATURES {
                features.insert(format!("f{feature}"), value(row, feature).into());
            }
            features.insert("score".to_owned(), 1.2.into());
            let features = serde_json::Value::Object(features);
            loop {
                match model.observe().drift(&features, None) {
                    Ok(()) => break,
                    Err(error) if error.code() == "WYRD_CLIENT_429_QUEUE_FULL" => {
                        state.flush().await?;
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        }
        state.shutdown().await?;
        let end = Utc::now() + chrono::TimeDelta::seconds(1);
        let deadline = tokio::time::Instant::now() + SETTLE;
        while self
            .count("SELECT COUNT(*) AS n FROM vala.drift.observations")
            .await?
            < u64::from(SAMPLES) * ROWS_PER_SAMPLE
        {
            if tokio::time::Instant::now() > deadline {
                return Err("the seeded Drift window never became queryable".into());
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        Ok((start - chrono::TimeDelta::seconds(1), end))
    }
}

/// Waits until `kind`'s Verifier `root` has a fitted baseline.
///
/// # Errors
///
/// Returns a read failure, a failed fit, or a fit outlasting [`SETTLE`].
async fn await_fit(cards: &Cards, kind: Kind, root: CardRef) -> Result<()> {
    let deadline = tokio::time::Instant::now() + SETTLE;
    let selector = CardSelector::exact(root);
    loop {
        let state = cards
            .get(selector.clone())
            .await?
            .status
            .and_then(|status| status.verification)
            .and_then(|verification| verification.baseline)
            .map(|baseline| baseline.state.as_str().to_owned());
        match state.as_deref() {
            Some("ready") => return Ok(()),
            Some("failed") => return Err(format!("{kind:?} baseline failed").into()),
            _ if tokio::time::Instant::now() > deadline => {
                return Err(format!("{kind:?} baseline never fitted").into());
            }
            _ => tokio::time::sleep(Duration::from_millis(500)).await,
        }
    }
}

/// The UID a registered or hydrated Card carries.
///
/// # Errors
///
/// Returns an error when it carries none.
fn uid(uid: &Option<CardUid>) -> Result<CardUid> {
    uid.clone()
        .ok_or_else(|| "a registered Card carries its UID".into())
}

/// A public client at `url` presenting `credential`.
///
/// # Errors
///
/// Returns the client configuration failure.
fn connect(url: &str, credential: &SecretString) -> Result<WyrdClient> {
    Ok(WyrdClient::with_config(ClientConfig {
        credential: Some(SecretString::from(credential.expose_secret().to_owned())),
        ..ClientConfig::from_global_with_overrides(&GlobalConfig::default(), Some(url), None)
    })?)
}

/// Writes the baseline Parquet artifact under `directory` and returns its
/// bytes: [`BASELINE_ROWS`] rows of [`FEATURES`] float features.
///
/// # Errors
///
/// Returns the batch, encoding, or write failure.
fn write_baseline(directory: &Path) -> Result<Vec<u8>> {
    let schema = Arc::new(Schema::new(
        (0..FEATURES)
            .map(|feature| Field::new(format!("f{feature}"), DataType::Float64, false))
            .collect::<Vec<_>>(),
    ));
    let columns: Vec<ArrayRef> = (0..FEATURES)
        .map(|feature| {
            Arc::new(Float64Array::from_iter_values(
                (0..BASELINE_ROWS).map(|row| value(row, feature)),
            )) as ArrayRef
        })
        .collect();
    let batch = RecordBatch::try_new(Arc::clone(&schema), columns)?;
    let mut bytes = Vec::new();
    let mut writer = parquet::arrow::ArrowWriter::try_new(&mut bytes, schema, None)?;
    writer.write(&batch)?;
    writer.close()?;
    std::fs::create_dir_all(directory.join("data"))?;
    std::fs::write(directory.join("data/data.parquet"), &bytes)?;
    Ok(bytes)
}

/// Writes the tenant's Card graph into `directory`: every workload when
/// `measured`, otherwise the assertion Agent alone.
///
/// # Errors
///
/// Returns the baseline or file write failure.
fn write_graph(directory: &Path, measured: bool) -> Result<()> {
    let verifier = |name: &str, implementation: &str| {
        format!(
            "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: {name}\n  version: 1.0.0\n  space: default\nspec:\n  implementation:\n{implementation}"
        )
    };
    let distribution = |features: usize| {
        let names: Vec<String> = (0..features).map(|feature| format!("f{feature}")).collect();
        format!(
            "      signal:\n        kind: Distribution\n        baseline_ref: {{kind: Data, name: capacity-baseline, version: 1.0.0, space: default}}\n        features: [{}]\n      condition:\n        kind: Statistical\n",
            names.join(", ")
        )
    };
    let agent = |alias: &str| {
        format!(
            "apiVersion: wyrd/v1\nkind: Agent\nmetadata:\n  name: capacity-{alias}-agent\n  version: 1.0.0\n  space: default\nspec:\n  prompt: ./agent-prompt.yaml\n  run_config:\n    max_iterations: 1\n"
        )
    };
    let binding = |alias: &str, verifier: &str| {
        format!(
            "    - alias: {alias}\n      ref: ./{alias}-agent.yaml\n      verified_by:\n        - verifier: ./{verifier}\n          runs_on: {{kind: observations_ready}}\n"
        )
    };
    let mut files = vec![
        (
            "agent-prompt.yaml".to_owned(),
            "apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  name: capacity-agent-prompt\n  version: 1.0.0\n  space: default\nspec:\n  provider: openai\n  model: gpt-test\n  messages: [answer the question]\n".to_owned(),
        ),
        ("assert-agent.yaml".to_owned(), agent("assert")),
        (
            "assert.yaml".to_owned(),
            verifier(
                "capacity-assert",
                "    kind: eval\n    spec:\n      pass_gate: {kind: all_pass}\n      tasks:\n        a: {kind: assertion, id: a, context_path: $.a, operator: equals, expected: \"yes\"}\n        b: {kind: assertion, id: b, context_path: $.b, operator: equals, expected: \"yes\"}\n        c: {kind: assertion, id: c, context_path: $.c, operator: equals, expected: \"yes\"}\n        d: {kind: assertion, id: d, context_path: $.d, operator: equals, expected: \"yes\"}\n",
            ),
        ),
    ];
    let mut components = binding("assert", "assert.yaml");
    if measured {
        let bytes = write_baseline(directory)?;
        let hex = format!("{:x}", sha2::Sha256::digest(&bytes));
        let digest = base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(&bytes));
        let columns: String = (0..FEATURES)
            .map(|feature| format!("      - name: f{feature}\n        dtype: float64\n"))
            .collect();
        files.push((
            "baseline.yaml".to_owned(),
            format!(
                "apiVersion: wyrd/v1\nkind: Data\nmetadata:\n  name: capacity-baseline\n  version: 1.0.0\n  space: default\nspec:\n  interface:\n    kind: Parquet\n    meta:\n      compression: Snappy\n  schema:\n    columns:\n{columns}  card_refs: []\n  stats:\n    row_count: {BASELINE_ROWS}\n    col_count: {FEATURES}\n    byte_count: {len}\n    sha256: {hex}\nartifacts:\n  - relative_path: data/data.parquet\n    sha256: {digest}\n    size_bytes: {len}\n    content_type: application/vnd.apache.parquet\n",
                len = bytes.len()
            ),
        ));
        files.push((
            "psi.yaml".to_owned(),
            verifier(
                "capacity-psi",
                &format!(
                    "    kind: drift\n    spec:\n      method: Psi\n{}      profile:\n        kind: Psi\n        binning_strategy: {{kind: Quantile, n_bins: 10}}\n        threshold: {{kind: Fixed, value: 0.25}}\n",
                    distribution(FEATURES)
                ),
            ),
        ));
        files.push((
            "spc.yaml".to_owned(),
            verifier(
                "capacity-spc",
                &format!(
                    "    kind: drift\n    spec:\n      method: Spc\n{}      profile:\n        kind: Spc\n        sample_size: 5\n",
                    distribution(SPC_FEATURES)
                ),
            ),
        ));
        files.push((
            "custom.yaml".to_owned(),
            verifier(
                "capacity-custom",
                "    kind: drift\n    spec:\n      method: Custom\n      signal:\n        kind: Metric\n        name: score\n      condition:\n        kind: Statistical\n      profile:\n        kind: Custom\n        metric_name: score\n        baseline_value: 1.0\n        alert_threshold: 0.5\n",
            ),
        ));
        files.push(("judge-prompt.json".to_owned(), judge_prompt()?));
        files.push((
            "judge.yaml".to_owned(),
            verifier(
                "capacity-judge",
                "    kind: eval\n    spec:\n      pass_gate: {kind: all_pass}\n      tasks:\n        answer: {kind: assertion, id: answer, context_path: $.answer, operator: equals, expected: \"yes\"}\n        judge:\n          kind: llm_judge\n          id: judge\n          judge_ref: {prompt: ./judge-prompt.json, tool_names: [], run_config: {max_iterations: 1}}\n          context_path: $.answer\n          operator: equals\n          expected: {passed: true}\n          max_retries: 0\n",
            ),
        ));
        files.push(("judge-agent.yaml".to_owned(), agent("judge")));
        files.push((
            "model.yaml".to_owned(),
            "apiVersion: wyrd/v1\nkind: Model\nmetadata:\n  name: capacity-model\n  version: 1.0.0\n  space: default\nspec:\n  interface:\n    kind: Custom\n    meta:\n      framework_version: 0.1.0\n      loader_module: fixture\n      loader_class: TinyModel\n      extra: {}\n  task_type: Other\n  signature:\n    inputs:\n      - name: f0\n        dtype: float64\n    outputs:\n      - name: score\n        dtype: float64\n  card_refs: []\n".to_owned(),
        ));
        components.push_str("    - alias: model\n      ref: ./model.yaml\n");
        components.push_str(&binding("judge", "judge.yaml"));
    }
    files.push((
        "service.yaml".to_owned(),
        format!(
            "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: capacity-service\n  version: 1.0.0\n  space: default\nspec:\n  service_type: agent\n  components:\n{components}"
        ),
    ));
    for (name, body) in files {
        std::fs::write(directory.join(name), body)?;
    }
    Ok(())
}

/// The judge Prompt Card: native `OpenAI` Chat grading `${answer}` with a
/// JSON-schema `{passed: bool}` response.
///
/// # Errors
///
/// Returns the Prompt construction failure.
fn judge_prompt() -> Result<String> {
    let prompt = skald_prompt::openai_chat(
        "gpt-test",
        skald_prompt::OpenAiChatOptions {
            messages: vec!["Grade the answer ${answer}.".to_owned()],
            variables: vec!["answer".to_owned()],
            output: Some(skald_prompt::ResponseFormat::json_schema(
                "judge_result",
                serde_json::json!({
                    "type": "object",
                    "properties": { "passed": { "type": "boolean" } },
                    "required": ["passed"],
                    "additionalProperties": false
                }),
            )?),
            ..skald_prompt::OpenAiChatOptions::default()
        },
    )?;
    Ok(serde_json::json!({
        "apiVersion": "wyrd/v1",
        "kind": "Prompt",
        "metadata": { "name": "capacity-judge", "version": "1.0.0", "space": "default" },
        "spec": prompt.into_native(),
    })
    .to_string())
}
