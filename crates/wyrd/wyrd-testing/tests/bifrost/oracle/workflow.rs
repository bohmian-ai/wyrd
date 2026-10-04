//! Tier-1 journey: an accepted Workflow's `bifrost.query` call is forwarded
//! to a remote Oracle and settles there before the run ends.
//!
//! The Workflow is submitted to the Scribe-only pod, which owns no Oracle, so
//! its built-in query tool reaches the elected Oracle leader through ordinary
//! authenticated forwarding, and the leader runs a distributed graph whose
//! follower is held at a real execute boundary. An explicit cancel and the
//! loss of the held follower pod must leave the leader's graph cleanup in
//! charge of the result: the run stays non-terminal and the model sees nothing
//! while that cleanup is paused. The run deadline is the query's own bound, so
//! the run times out at it without waiting on that cleanup, never earlier, and
//! the model still sees no result.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use serde_json::{Value, json};
use tokio::sync::watch;
use wyrd_client::cards::Cards;
use wyrd_server::config::BifrostTarget;
use wyrd_spec::card::workflow::{WorkflowRun, WorkflowRunStatus};
use wyrd_spec::storage::IDEMPOTENCY_KEY_HEADER;

use crate::peer_cluster::PeerCluster;
use crate::support::{JourneyError, await_baseline, public_client};

/// Pod that leads every forwarded query: forwarding elects the lowest
/// eligible Oracle node identity, which is pod zero's.
const LEADER: usize = 0;

/// Pod holding the distributed graph's follower task at its execute pause.
const HELD_FOLLOWER: usize = 1;

/// Scribe-only pod the Workflow is submitted to; it forwards every query.
const INGRESS: usize = 3;

/// Header every `/v1` route reads the caller's access token from.
const ACCESS_TOKEN_HEADER: &str = "x-wyrd-access-token";

/// Run deadline of the deadline case, long enough to arm the cleanup pause
/// after the sibling query and before the deadline cancels the tool query.
const RUN_TIMEOUT_SECONDS: u64 = 20;

/// Bound for every wait on a run, a pause, or the upstream.
const PATIENCE: Duration = Duration::from_secs(60);

/// Interval between reads of a run or query state being waited on.
const POLL: Duration = Duration::from_millis(100);

/// How the held forwarded query is made to end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalCause {
    /// The run's owner cancels it.
    Cancel,
    /// The run's total deadline expires.
    Deadline,
    /// The held follower's pod disappears mid-graph.
    PodKill,
}

/// A forwarded Workflow query settles on its remote Oracle before the run
/// ends on a cancel or a pod loss, and a deadline ends the run exactly at the
/// query's own bound.
///
/// A cancel records a `cancelled` Analytical query; a deadline, which the
/// leader ends as a query timeout, and a pod loss record a `failed` one.
///
/// # Panics
///
/// Panics when any terminal cause breaks one of its claims.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn workflow_forwarded_query_settles_before_the_run_ends() {
    for cause in [
        TerminalCause::Cancel,
        TerminalCause::Deadline,
        TerminalCause::PodKill,
    ] {
        prove_forwarded_query_settles(cause)
            .await
            .unwrap_or_else(|error| panic!("{cause:?}: {error}"));
    }
}

/// Drives one clean topology through one terminal cause.
///
/// Every case starts its own cluster, so what one held query leaves behind
/// cannot be confused with another's.
///
/// # Errors
///
/// Returns the first claim that broke.
async fn prove_forwarded_query_settles(cause: TerminalCause) -> Result<(), JourneyError> {
    let upstream = Upstream::start().await?;
    let mut cluster = PeerCluster::start_with_gateway_provider_root(
        &[
            BifrostTarget::Oracle,
            BifrostTarget::Oracle,
            BifrostTarget::Oracle,
            BifrostTarget::Scribe,
        ],
        upstream.url.clone(),
    )
    .await?;
    let table = format!("workflow_forwarded_{}", uuid::Uuid::now_v7().simple());
    cluster.register_table(INGRESS, &table).await?;
    cluster.ingest_rows(INGRESS, &table, 0, 12, 3).await?;
    cluster.ingest_rows(INGRESS, &table, 0, 12, 3).await?;
    for index in 0..cluster.len() {
        cluster.refresh_snapshot(index).await?;
    }
    let sql = format!(
        "SELECT filter_key, COUNT(*) AS matched FROM vala.bifrost.{table} \
         GROUP BY filter_key ORDER BY filter_key"
    );

    let api_key = cluster
        .provision_public_api_key("workflow-forwarding")
        .await?;
    let ingress = Ingress::new(&cluster, &api_key).await?;
    ingress.deploy().await?;
    let bundle = query_workflow()?;
    Cards::with_client(public_client(cluster.server(INGRESS)?, &api_key)?)
        .register_from_path(&bundle.path().join("workflow.yaml"))
        .await?;

    upstream.reply(json!({
        "role": "assistant",
        "content": null,
        "tool_calls": [{
            "id": "forwarded",
            "type": "function",
            "function": { "name": "bifrost.query", "arguments": json!({ "sql": sql }).to_string() }
        }]
    }));
    if cause == TerminalCause::PodKill {
        upstream.reply(json!({ "role": "assistant", "content": "DONE" }));
    }

    let mut baseline = Vec::new();
    for index in cluster.indices_of(BifrostTarget::Oracle) {
        baseline.push((index, cluster.ownership_snapshot(index)?));
    }
    cluster.arm_execute_pause(HELD_FOLLOWER)?;
    let timeout = (cause == TerminalCause::Deadline).then_some(RUN_TIMEOUT_SECONDS);
    let run = ingress.create(timeout).await?;
    cluster.await_execute_paused(HELD_FOLLOWER).await?;

    // Ordinary queries stay serviceable while the Workflow's query holds a
    // follower task of its graph.
    let sibling = cluster.execute_sql(LEADER, &sql).await?;
    if sibling != 3 {
        return Err(format!("the sibling query returned {sibling} rows, expected 3").into());
    }

    let success = duration_outcome("success");
    let expected_outcome = duration_outcome(cause.recorded_outcome());
    let successes_before = cluster.metric_totals_labeled(&[DURATION], &success)?[DURATION];
    let outcomes_before = cluster.metric_totals_labeled(&[DURATION], &expected_outcome)?[DURATION];

    cluster.arm_cleanup_pause();
    let cancel = match cause {
        TerminalCause::Cancel => {
            let ingress = ingress.clone();
            let run_id = run.clone();
            Some(tokio::spawn(async move { ingress.cancel(&run_id).await }))
        }
        TerminalCause::Deadline => None,
        TerminalCause::PodKill => {
            cluster.kill(HELD_FOLLOWER).await?;
            None
        }
    };
    cluster.await_cleanup_paused().await?;

    // The leader's cleanup still owns the query, so its terminal has not
    // reached the forwarding ingress: the model cannot have seen a tool
    // result, and unless the query's own deadline has passed, the run cannot
    // have ended.
    let held = ingress.get(&run).await?;
    let cancel_finished = cancel
        .as_ref()
        .is_some_and(tokio::task::JoinHandle::is_finished);
    let arrivals = upstream.arrivals();
    cluster.release_cleanup_pause();
    if cause != TerminalCause::Deadline && (held.status.is_terminal() || cancel_finished) {
        return Err(format!("the run ended before its query settled: {held:?}").into());
    }
    if arrivals != 1 {
        return Err(format!("the model saw a tool result before settlement: {arrivals}").into());
    }

    let terminal = match cancel {
        Some(cancel) => cancel.await??,
        None => ingress.terminal(&run).await?,
    };
    let expected = match cause {
        TerminalCause::Cancel => WorkflowRunStatus::Cancelled,
        TerminalCause::Deadline => WorkflowRunStatus::TimedOut,
        TerminalCause::PodKill => WorkflowRunStatus::Succeeded,
    };
    if terminal.status != expected {
        return Err(format!("expected a {expected:?} run, saw {terminal:?}").into());
    }
    // The tool query's deadline is the run's remaining time, so a run that
    // timed out earlier than its own deadline would have cut its query short.
    if cause == TerminalCause::Deadline {
        let bound = terminal.created_at
            + chrono::Duration::from_std(Duration::from_secs(RUN_TIMEOUT_SECONDS))?;
        if terminal.ended_at.is_none_or(|ended| ended < bound) || !terminal.outputs.is_empty() {
            return Err(format!(
                "the run must time out at its deadline with no result: {terminal:?}"
            )
            .into());
        }
    }
    if cause == TerminalCause::PodKill {
        let results = upstream.calls()[1]["messages"].to_string();
        if !results.contains("WYRD_VALA_") || results.contains("columns") {
            return Err(format!("the lost query must fail without rows: {results}").into());
        }
    }

    let successes = cluster.metric_totals_labeled(&[DURATION], &success)?[DURATION];
    let outcomes = cluster.metric_totals_labeled(&[DURATION], &expected_outcome)?[DURATION];
    if successes != successes_before {
        return Err("the held query must not record a successful Analytical query".into());
    }
    if (outcomes - outcomes_before - 1.0).abs() > f64::EPSILON {
        return Err(format!("the held query must record exactly one {expected_outcome:?}").into());
    }

    // Once the query has settled, every Oracle drains back to its baseline
    // ownership and the topology answers again; the killed pod no longer
    // serves the graph's follower stages.
    if cause != TerminalCause::PodKill {
        for (index, before) in baseline {
            await_baseline(&cluster, index, before).await?;
        }
        let later = cluster.execute_sql(LEADER, &sql).await?;
        if later != 3 {
            return Err(format!("the later query returned {later} rows, expected 3").into());
        }
    }
    cluster.shutdown().await?;
    Ok(())
}

/// Production Oracle query duration family, labelled by class and outcome.
const DURATION: &str = "oracle_query_duration_seconds";

/// Analytical-class [`DURATION`] labels for one `outcome`.
fn duration_outcome(outcome: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("class".to_owned(), "analytical".to_owned()),
        ("outcome".to_owned(), outcome.to_owned()),
    ])
}

impl TerminalCause {
    /// The [`DURATION`] outcome the held query's end must record.
    ///
    /// A cancel reaches the leader as a registry cancel, which is recorded as
    /// `cancelled` even before the query's stream exists. A deadline ends the
    /// leader's first-batch wait as a query timeout, and that end before a
    /// stream exists is recorded as `failed`, as is a lost follower.
    const fn recorded_outcome(self) -> &'static str {
        match self {
            Self::Cancel => "cancelled",
            Self::Deadline | Self::PodKill => "failed",
        }
    }
}

/// The Workflow-run and gateway routes of the ingress pod, as its admin.
#[derive(Clone)]
struct Ingress {
    /// Plain HTTP client for the public routes.
    http: reqwest::Client,
    /// Ingress pod base URL.
    base: String,
    /// Access token of the tenant administrator.
    token: String,
}

impl Ingress {
    /// Exchange `api_key` at the ingress pod.
    ///
    /// # Errors
    ///
    /// Returns a message when the pod serves no HTTP listener, or the
    /// exchange failure.
    async fn new(
        cluster: &PeerCluster,
        api_key: &secrecy::SecretString,
    ) -> Result<Self, JourneyError> {
        let server = cluster.server(INGRESS)?;
        Ok(Self {
            http: reqwest::Client::new(),
            base: server
                .base_url()
                .ok_or("the ingress pod serves no public HTTP listener")?
                .to_owned(),
            token: server.exchange_api_key(api_key).await?,
        })
    }

    /// Submit the provider key and one `openai/gpt-5-5` deployment.
    ///
    /// # Errors
    ///
    /// Returns a message naming the administration route that refused.
    async fn deploy(&self) -> Result<(), JourneyError> {
        for (route, body) in [
            (
                "provider-credentials/openai-key",
                json!({
                    "name": "openai-key",
                    "provider": "openai",
                    "source": { "managed_secret": { "secret": "sk-workflow-forwarding" } },
                }),
            ),
            (
                "provider-deployments/gpt-5-5",
                json!({
                    "name": "gpt-5-5",
                    "model": { "provider": "openai", "model": "gpt-5-5" },
                    "adapter": "openai",
                    "auth": { "bearer": { "credential": "openai-key" } },
                    "capabilities": ["chat_completions"],
                    "routing_weight": 1,
                }),
            ),
        ] {
            let response = self
                .authorized(
                    self.http
                        .put(format!("{}/v1/admin/gateway/{route}", self.base)),
                )
                .json(&body)
                .send()
                .await?;
            let status = response.status();
            if !status.is_success() {
                return Err(format!("{route}: {status} {}", response.text().await?).into());
            }
        }
        Ok(())
    }

    /// Accept one run of the query Workflow with an optional total timeout.
    ///
    /// # Errors
    ///
    /// Returns a message when the create is not accepted with `202`.
    async fn create(&self, timeout_seconds: Option<u64>) -> Result<String, JourneyError> {
        let response = self
            .authorized(self.http.post(format!("{}/v1/workflow-runs", self.base)))
            .header(IDEMPOTENCY_KEY_HEADER, uuid::Uuid::now_v7().to_string())
            .json(&json!({
                "workflow": {
                    "kind": "Workflow",
                    "name": "forwarded-query",
                    "version": "1.0.0",
                    "space": "engineering",
                },
                "timeout_seconds": timeout_seconds,
            }))
            .send()
            .await?;
        let status = response.status();
        let body = response.text().await?;
        if status != reqwest::StatusCode::ACCEPTED {
            return Err(format!("the run was not accepted: {status} {body}").into());
        }
        let run: WorkflowRun = serde_json::from_str(&body)?;
        Ok(run.run_id.to_string())
    }

    /// The run's current snapshot.
    ///
    /// # Errors
    ///
    /// Returns a message when the read is refused.
    async fn get(&self, run_id: &str) -> Result<WorkflowRun, JourneyError> {
        let request = self
            .http
            .get(format!("{}/v1/workflow-runs/{run_id}", self.base));
        self.run(request).await
    }

    /// Cancel the run and return the terminal snapshot that won.
    ///
    /// # Errors
    ///
    /// Returns a message when the cancel is refused.
    async fn cancel(&self, run_id: &str) -> Result<WorkflowRun, JourneyError> {
        let request = self
            .http
            .post(format!("{}/v1/workflow-runs/{run_id}/cancel", self.base));
        self.run(request).await
    }

    /// Read the run until it is terminal.
    ///
    /// # Errors
    ///
    /// Returns a message when a read is refused or the run is still active
    /// after [`PATIENCE`].
    async fn terminal(&self, run_id: &str) -> Result<WorkflowRun, JourneyError> {
        let deadline = tokio::time::Instant::now() + PATIENCE;
        loop {
            let run = self.get(run_id).await?;
            if run.status.is_terminal() {
                return Ok(run);
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(format!("the run never ended: {run:?}").into());
            }
            tokio::time::sleep(POLL).await;
        }
    }

    /// Send one run route request and decode its `200` snapshot.
    ///
    /// # Errors
    ///
    /// Returns a message when the route answers anything but `200`.
    async fn run(&self, request: reqwest::RequestBuilder) -> Result<WorkflowRun, JourneyError> {
        let response = self.authorized(request).send().await?;
        let status = response.status();
        let body = response.text().await?;
        if status != reqwest::StatusCode::OK {
            return Err(format!("the run route refused: {status} {body}").into());
        }
        Ok(serde_json::from_str(&body)?)
    }

    /// Attach the administrator's access token.
    fn authorized(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        request.header(ACCESS_TOKEN_HEADER, format!("Bearer {}", self.token))
    }
}

/// Write the one-step Workflow `forwarded-query` whose Agent declares only
/// `bifrost.query` and routes through the governed gateway.
///
/// # Errors
///
/// Returns the temporary directory or file write failure.
fn query_workflow() -> Result<tempfile::TempDir, JourneyError> {
    let bundle = tempfile::TempDir::new()?;
    let write = |file: &str, body: &str| std::fs::write(bundle.path().join(file), body);
    write(
        "prompt.yaml",
        "apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  space: engineering\n  name: forwarded-query-prompt\n  version: \"1.0.0\"\nspec:\n  model: gpt-5-5\n  request:\n    model: gpt-5-5\n    messages:\n      - role: system\n        content: \"You count the fixture groups.\"\n      - role: user\n        content: \"Count them.\"\n  response_type: text\n",
    )?;
    write(
        "agent.yaml",
        "apiVersion: wyrd/v1\nkind: Agent\nmetadata:\n  space: engineering\n  name: forwarded-query-agent\n  version: \"1.0.0\"\nspec:\n  prompt: ./prompt.yaml\n  tool_names: [bifrost.query]\n  run_config:\n    max_iterations: 4\n",
    )?;
    write(
        "workflow.yaml",
        "apiVersion: wyrd/v1\nkind: Workflow\nmetadata:\n  space: engineering\n  name: forwarded-query\n  version: \"1.0.0\"\nspec:\n  llm_route:\n    kind: wyrd_gateway\n  steps:\n    - id: count\n      action:\n        type: agent\n        target: ./agent.yaml\n  outputs:\n    answer: steps.count.output.text\n",
    )?;
    Ok(bundle)
}

/// Shared state of the scripted model upstream.
struct Script {
    /// Every request body in arrival order.
    calls: Mutex<Vec<Value>>,
    /// Number of requests received so far.
    arrivals: watch::Sender<usize>,
    /// Assistant messages answered in order.
    replies: Mutex<VecDeque<Value>>,
}

/// Local OpenAI-compatible upstream serving `POST /v1/chat/completions`.
///
/// A request arriving after the scripted replies are exhausted is held open
/// until its caller abandons it, so a run past its last scripted answer
/// stays in flight until it is cancelled or times out.
struct Upstream {
    /// Origin every pod's gateway adapters are rooted at.
    url: url::Url,
    /// State shared with the serving task.
    script: Arc<Script>,
}

impl Upstream {
    /// Bind a loopback listener and serve the scripted completions on it.
    ///
    /// # Errors
    ///
    /// Returns the bind failure.
    async fn start() -> Result<Self, JourneyError> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let url = url::Url::parse(&format!("http://{}", listener.local_addr()?))?;
        let script = Arc::new(Script {
            calls: Mutex::default(),
            arrivals: watch::Sender::new(0),
            replies: Mutex::default(),
        });
        let app = axum::Router::new()
            .route("/v1/chat/completions", axum::routing::post(complete))
            .with_state(Arc::clone(&script));
        tokio::spawn(async move { axum::serve(listener, app).await });
        Ok(Self { url, script })
    }

    /// Queue one assistant message as the next answer.
    fn reply(&self, message: Value) {
        self.script
            .replies
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_back(message);
    }

    /// Every request body received so far.
    fn calls(&self) -> Vec<Value> {
        self.script
            .calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Number of requests received so far.
    fn arrivals(&self) -> usize {
        *self.script.arrivals.borrow()
    }
}

/// Record one completion request and answer the next scripted message, or
/// hold the request open when none is left.
async fn complete(State(script): State<Arc<Script>>, Json(body): Json<Value>) -> Json<Value> {
    script
        .calls
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(body);
    script.arrivals.send_modify(|arrived| *arrived += 1);
    let next = script
        .replies
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .pop_front();
    let Some(message) = next else {
        return std::future::pending().await;
    };
    let finish_reason = if message.get("tool_calls").is_some() {
        "tool_calls"
    } else {
        "stop"
    };
    Json(json!({
        "id": "chatcmpl-forwarded",
        "object": "chat.completion",
        "created": 0,
        "model": "gpt-5-5",
        "choices": [{ "index": 0, "message": message, "finish_reason": finish_reason }],
        "usage": { "prompt_tokens": 5, "completion_tokens": 2, "total_tokens": 7 }
    }))
}
