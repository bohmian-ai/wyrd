//! The support desk: an Agent-backed Service that answers customer questions,
//! records each ticket, and is verified continuously and in real time.
//!
//! [`deploy`] registers the checked-in Cards under `service/`, which ensures
//! the declared `vala.datasets.tickets` table, turns on metadata capture, and
//! checks the Agent's model is deployed. [`serve`] answers every question in
//! its own Agent Run. [`wait_for_verdicts`] waits for both Verifiers to judge
//! every answer, and [`explain`] joins one Run's evidence over MCP.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::Instrument as _;
use wyrd_sdk::bifrost::QueryParam;
use wyrd_sdk::cards::{CardSelector, Cards, HydrationMode, RegistrationReceipt};
use wyrd_sdk::gateway::{GatewayCaptureMode, GatewayCapturePolicyWrite};
use wyrd_sdk::otel::start_telemetry;
use wyrd_sdk::state::WyrdState;
use wyrd_sdk::{Bifrost, Gateway, Spec, WyrdClient, WyrdError};

/// The failure any support-desk step returns.
pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// How many requests [`serve`] answers; every tenth asks for a refund.
pub const REQUESTS: usize = 100;

/// The checked-in Service Cards.
#[must_use]
pub fn service_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/support-desk/service")
}

/// The deployed support desk: its registration and loaded Card graph.
pub struct Desk {
    /// Every Card the Service registered.
    pub receipt: RegistrationReceipt,
    /// The hydrated Service.
    pub state: WyrdState,
}

impl Desk {
    /// The registered UID of the Card named `name`.
    ///
    /// # Panics
    /// Panics when the Service registered no such Card.
    #[must_use]
    pub fn uid(&self, name: &str) -> String {
        self.receipt
            .outcomes
            .iter()
            .find(|outcome| outcome.card_ref.name.as_str() == name)
            .and_then(|outcome| outcome.card_ref.uid.as_ref())
            .unwrap_or_else(|| panic!("{name} is registered"))
            .to_string()
    }
}

/// One answered request.
#[derive(Debug)]
pub struct Served {
    /// The request's application Run.
    pub run_id: String,
    /// Whether `no-refund-promise` passed the answer in real time.
    pub passed: bool,
}

/// One `vala.datasets.tickets` row.
#[derive(Serialize)]
struct Ticket<'a> {
    /// The request's ticket.
    ticket_id: String,
    /// The customer's question.
    question: &'a str,
    /// The Agent's answer.
    answer: &'a str,
    /// Whether the question asked for a refund.
    refund: bool,
}

/// The context both Verifiers judge.
#[derive(Serialize)]
struct Answer<'a> {
    /// The Agent's answer.
    answer: &'a str,
}

/// Register and hydrate the Service into `bundle`, capture gateway call
/// metadata, and check the Agent's model is deployed.
///
/// # Errors
/// Returns the registration refusal, including
/// `WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH` for an incompatible existing
/// `tickets` table, and `WYRD_GATEWAY_404_MODEL_UNAVAILABLE` naming the
/// model when no deployment serves it.
pub async fn deploy(client: &WyrdClient, bundle: &Path) -> Result<Desk, Error> {
    let cards = Cards::with_client(client.clone());
    let receipt = cards
        .register_from_path(service_dir().join("support-desk.yaml"))
        .await?;
    cards
        .hydrate(
            CardSelector::exact(receipt.root.clone()),
            bundle,
            HydrationMode::Complete,
        )
        .await?;
    let state = WyrdState::from_path_with_client(bundle, client.clone())?;
    let gateway = Gateway::with_client(client.clone());
    gateway
        .put_capture_policy(&GatewayCapturePolicyWrite {
            mode: GatewayCaptureMode::Metadata,
            payload_fields: Default::default(),
        })
        .await?;
    let Spec::Prompt(prompt) = &state.card("prompt")?.spec else {
        return Err("the `prompt` component is not a Prompt".into());
    };
    let model = &prompt.prompt.model;
    if !gateway
        .deployments()
        .await?
        .iter()
        .any(|deployment| deployment.model.model.as_str() == model)
    {
        return Err(WyrdError::GatewayModelUnavailable {
            message: format!("the support Agent's model `{model}` is not deployed"),
            details: json!({ "model": model }),
        }
        .into());
    }
    Ok(Desk { receipt, state })
}

/// The customer question of request `index`.
#[must_use]
pub fn question(index: usize) -> String {
    if index % 10 == 0 {
        format!("Can I get a refund for order {index}?")
    } else {
        format!("Where is order {index}?")
    }
}

/// Answer [`REQUESTS`] questions, each in its own Agent Run inside a
/// `support-desk.request` span: invoke the Agent through the gateway, record
/// the ticket, observe the answer for `answer-quality`, and judge it with
/// `no-refund-promise`. Telemetry and observations are flushed on return.
///
/// # Errors
/// Returns the first refused step.
pub async fn serve(desk: &Desk) -> Result<Vec<Served>, Error> {
    let state = &desk.state;
    state.start_bifrost().await?;
    let telemetry = start_telemetry(state)?;
    let mut served = Vec::with_capacity(REQUESTS);
    for index in 0..REQUESTS {
        let run = state.run_for_card("agent")?;
        let question = question(index);
        let request = async {
            let answer = run.invoke(&[("question", &question)]).await?;
            let observe = run.observe();
            observe
                .record(
                    "vala.datasets.tickets",
                    &Ticket {
                        ticket_id: format!("T-{index}"),
                        question: &question,
                        answer: &answer,
                        refund: question.contains("refund"),
                    },
                )
                .await?;
            observe.eval(&Answer { answer: &answer }, Default::default())?;
            let judgment = observe
                .verify("no-refund-promise", &Answer { answer: &answer })
                .await?;
            Ok::<_, WyrdError>(judgment.passed())
        };
        let passed = run
            .scope(request.instrument(tracing::info_span!("support-desk.request")))
            .await?;
        served.push(Served {
            run_id: run.run_id().to_string(),
            passed,
        });
    }
    telemetry.shutdown()?;
    state.shutdown().await?;
    Ok(served)
}

/// Verdict counts per Verifier name: `(passed, failed)`.
pub type Verdicts = BTreeMap<String, (u64, u64)>;

/// One verdict count row.
#[derive(Deserialize)]
struct VerdictCount {
    /// The Verifier Card.
    card_uid: String,
    /// `passed`, `failed`, or `inconclusive`.
    verdict: String,
    /// Results with that verdict.
    n: i64,
}

/// Wait until both Verifiers have judged every answer, then return their
/// verdict counts.
///
/// # Errors
/// Returns a query refusal, or a timeout when the verdicts are not all in
/// within `timeout`.
pub async fn wait_for_verdicts(
    client: &WyrdClient,
    desk: &Desk,
    timeout: Duration,
) -> Result<Verdicts, Error> {
    let bifrost = Bifrost::connect(client).await?;
    let names = ["answer-quality", "no-refund-promise"].map(|name| (desk.uid(name), name));
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let rows: Vec<VerdictCount> = bifrost
            .sql_as(
                "SELECT card_uid, verdict, COUNT(*) AS n FROM vala.verification.results \
                 WHERE subject_card_uid = $1 GROUP BY card_uid, verdict",
                &[QueryParam::String(desk.uid("support-agent"))],
            )
            .await?;
        let mut verdicts = Verdicts::new();
        for row in rows {
            if let Some((_, name)) = names.iter().find(|(uid, _)| *uid == row.card_uid) {
                let counts = verdicts.entry((*name).to_owned()).or_default();
                let n = u64::try_from(row.n)?;
                match row.verdict.as_str() {
                    "passed" => counts.0 += n,
                    _ => counts.1 += n,
                }
            }
        }
        let judged = |name: &str| {
            verdicts
                .get(name)
                .map_or(0, |(passed, failed)| passed + failed)
        };
        if names
            .iter()
            .all(|(_, name)| judged(name) >= REQUESTS as u64)
        {
            return Ok(verdicts);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(
                format!("verdicts still incomplete after {timeout:?}: {verdicts:?}").into(),
            );
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// One Run's joined evidence: its observation, ticket, request span, gateway
/// call, and both verdicts, as column name to value.
pub type Explanation = BTreeMap<String, Value>;

/// Join every piece of evidence of Run `run_id` through MCP `bifrost.query`,
/// with a fresh access token.
///
/// # Errors
/// Returns a token, transport, or tool failure, or an error when the Run has
/// no complete joined row.
pub async fn explain(client: &WyrdClient, run_id: &str) -> Result<Explanation, Error> {
    uuid_like(run_id)?;
    let sql = format!(
        "SELECT o.run_id, o.trace_id, t.ticket_id, t.answer, s.name AS span, \
         g.call_id, g.card_uid AS call_card_uid, c.verdict AS continuous, r.verdict AS realtime \
         FROM vala.eval.observations o \
         JOIN vala.datasets.tickets t ON t.run_id = o.run_id \
         JOIN vala.traces.spans s ON s.trace_id = o.trace_id AND s.run_id = o.run_id \
         JOIN vala.gateway.calls g ON g.run_id = o.run_id AND g.card_uid = o.card_uid \
         JOIN vala.verification.results c ON c.run_id = o.run_id AND c.binding_id IS NOT NULL \
         JOIN vala.verification.results r ON r.run_id = o.run_id AND r.binding_id IS NULL \
         WHERE o.run_id = '{run_id}' AND s.name = 'support-desk.request'"
    );
    let token = client.access_token().await?;
    let reply = reqwest::Client::new()
        .post(format!("{}/mcp", client.server_url()))
        .header("x-wyrd-access-token", format!("Bearer {}", token.expose()))
        .header("mcp-protocol-version", MCP_VERSION)
        .header("mcp-method", "tools/call")
        .header("mcp-name", "bifrost.query")
        .header("accept", "application/json, text/event-stream")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "bifrost.query",
                "arguments": { "sql": sql },
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": MCP_VERSION,
                    "io.modelcontextprotocol/clientInfo": { "name": "support-desk", "version": "1.0.0" },
                    "io.modelcontextprotocol/clientCapabilities": {},
                },
            },
        }))
        .send()
        .await?;
    let status = reply.status();
    let reply = reply.text().await?;
    if !status.is_success() {
        return Err(format!("MCP refused the call with {status}: {reply}").into());
    }
    let message: Value = reply
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .find_map(|data| serde_json::from_str(data.trim()).ok())
        .ok_or("the MCP reply carries no message")?;
    let result = &message["result"];
    if result["isError"] == true {
        return Err(format!("bifrost.query failed: {}", result["structuredContent"]).into());
    }
    let content = &result["structuredContent"];
    let row = content["rows"]
        .as_array()
        .and_then(|rows| rows.first())
        .and_then(Value::as_array)
        .ok_or_else(|| format!("Run {run_id} has no joined evidence"))?;
    Ok(content["columns"]
        .as_array()
        .ok_or("the result carries its columns")?
        .iter()
        .zip(row)
        .map(|(column, value)| {
            (
                column["name"].as_str().unwrap_or_default().to_owned(),
                value.clone(),
            )
        })
        .collect())
}

/// The MCP protocol revision Wyrd serves.
const MCP_VERSION: &str = "2026-07-28";

/// Refuse a Run ID that is not hex and hyphens before it is quoted into SQL.
///
/// # Errors
/// Returns an error naming the malformed Run ID.
fn uuid_like(run_id: &str) -> Result<(), Error> {
    if run_id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        Ok(())
    } else {
        Err(format!("malformed Run ID {run_id}").into())
    }
}
