//! The support desk: an Agent-backed Service that answers customer questions,
//! records each ticket, and checks every answer against its refund policy
//! with the `support/policy-check` Verifier.
//!
//! [`deploy`] registers the checked-in Cards under `service/`, which ensures
//! the declared `vala.datasets.tickets` table, turns on metadata capture, and
//! checks the Agent's model is deployed. [`serve`] answers every question in
//! its own Agent Run and submits each answer for a background policy check.
//! [`wait_for_verdicts`] resumes each check from its execution ID alone and
//! waits for its result, [`check`] judges one answer immediately, and
//! [`explain`] joins one Run's evidence over MCP.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};
use tracing::Instrument as _;
use wyrd_ai::cards::{CardSelector, Cards, HydrationMode, RegistrationReceipt};
use wyrd_ai::gateway::{GatewayCaptureMode, GatewayCapturePolicyWrite, ModelId};
use wyrd_ai::observe::{File, Input, Inputs, Submission, VerificationExecution};
use wyrd_ai::otel::start_telemetry;
use wyrd_ai::state::WyrdState;
use wyrd_ai::{
    ExecutionId, ExecutionRoute, Gateway, Spec, VerificationResult, WyrdClient, WyrdError,
};

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
    /// The background policy check of the request's answer.
    pub execution_id: ExecutionId,
}

/// One `vala.datasets.tickets` row.
#[derive(Serialize)]
struct TicketRow<'a> {
    /// The request's ticket.
    ticket_id: String,
    /// The customer's question.
    question: &'a str,
    /// The Agent's answer.
    answer: &'a str,
    /// Whether the question asked for a refund.
    refund: bool,
}

/// The `policy-check` inputs for `answer`: the answer text and the
/// checked-in refund policy PDF.
fn policy_check(answer: &str) -> Inputs {
    Inputs::named([
        ("answer", Input::from(json!(answer))),
        (
            "policy",
            Input::from(File::path(service_dir().join("refund-policy.pdf"))),
        ),
    ])
}

/// Register and hydrate the Service into `bundle`, capture gateway call
/// metadata, and check the Agent's model is deployed.
///
/// # Errors
/// Returns the registration refusal, including
/// `WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH` for an incompatible existing
/// `tickets` table, and `WYRD_GATEWAY_404_MODEL_UNAVAILABLE` naming the exact
/// gateway deployment to configure when the Agent's gateway route names a
/// deployment that does not serve the Prompt's model.
pub async fn deploy(client: &WyrdClient, bundle: &Path) -> Result<Desk, Error> {
    let cards = Cards::connect_with(client).await?;
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
    let state = WyrdState::from_path_with(bundle, client.clone())?;
    let gateway = Gateway::connect_with(client).await?;
    gateway
        .put_capture_policy(&GatewayCapturePolicyWrite {
            mode: GatewayCaptureMode::Metadata,
            payload_fields: BTreeSet::default(),
        })
        .await?;
    let Spec::Prompt(prompt) = &state.card("prompt")?.spec else {
        return Err("the `prompt` component is not a Prompt".into());
    };
    let ExecutionRoute::WyrdGateway { deployment: name } =
        &state.agent("agent")?.card().spec.execution_route
    else {
        return Ok(Desk { receipt, state });
    };
    let model = &prompt.prompt.model;
    let served = ModelId::new(model)?;
    if !gateway
        .deployments()
        .await?
        .iter()
        .any(|deployment| deployment.name == *name && deployment.permits(&served))
    {
        return Err(WyrdError::GatewayModelUnavailable {
            message: format!(
                "the support Agent's model {name}/{model} is not deployed; configure the gateway \
                 deployment {name} (PUT /v1/admin/gateway/provider-deployments/{name})"
            ),
            details: json!({ "deployment": name, "model": model }),
        }
        .into());
    }
    Ok(Desk { receipt, state })
}

/// The customer question of request `index`.
#[must_use]
pub fn question(index: usize) -> String {
    if index.is_multiple_of(10) {
        format!("Can I get a refund for order {index}?")
    } else {
        format!("Where is order {index}?")
    }
}

/// Answer [`REQUESTS`] questions, each in its own Agent Run inside a
/// `support-desk.request` span: invoke the Agent through the gateway, record
/// the ticket, and submit the answer for a background `policy-check` without
/// waiting for it. Telemetry and tickets are flushed on return.
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
                    &TicketRow {
                        ticket_id: format!("T-{index}"),
                        question: &question,
                        answer: &answer,
                        refund: question.contains("refund"),
                    },
                )
                .await?;
            match observe
                .submit("policy-check", policy_check(&answer))
                .await?
            {
                Submission::Queued(execution) => Ok(*execution.id()),
                Submission::Recorded => Err(WyrdError::Internal {
                    message: "policy-check judges Run evidence, not a window".to_owned(),
                    details: json!({ "verifier": "policy-check" }),
                }),
            }
        };
        let execution_id = run
            .scope(request.instrument(tracing::info_span!("support-desk.request")))
            .await?;
        served.push(Served {
            run_id: run.run_id().to_string(),
            execution_id,
        });
    }
    telemetry.shutdown()?;
    state.shutdown().await?;
    Ok(served)
}

/// Judge `answer` with `policy-check` now, in a fresh Agent Run, and return
/// its result with the verdict and each requirement's outcome.
///
/// # Errors
/// Returns the local input refusal or the server's refusal of the execution.
pub async fn check(desk: &Desk, answer: &str) -> Result<VerificationResult, Error> {
    Ok(desk
        .state
        .run_for_card("agent")?
        .observe()
        .verify("policy-check", policy_check(answer))
        .await?)
}

/// Resume every served request's background check from its execution ID
/// alone, as another process would, and wait for its result; results are in
/// `served` order.
///
/// # Errors
/// Returns the execution's own error, or
/// `WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT` when the results are not all in
/// within `timeout`.
pub async fn wait_for_verdicts(
    client: &WyrdClient,
    served: &[Served],
    timeout: Duration,
) -> Result<Vec<VerificationResult>, Error> {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut results = Vec::with_capacity(served.len());
    for request in served {
        let execution = VerificationExecution::connect_with(request.execution_id, client).await?;
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        results.push(execution.wait(remaining).await?);
    }
    Ok(results)
}

/// One Run's joined evidence: its ticket, request span, gateway call, and
/// policy-check verdict, as column name to value.
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
        "SELECT t.run_id, s.trace_id, t.ticket_id, t.answer, s.name AS span, \
         g.call_id, g.card_uid AS call_card_uid, r.verdict \
         FROM vala.datasets.tickets t \
         JOIN vala.traces.spans s ON s.run_id = t.run_id \
         JOIN vala.gateway.calls g ON g.run_id = t.run_id \
         JOIN vala.verification.results r ON r.run_id = t.run_id \
         WHERE t.run_id = '{run_id}' AND s.name = 'support-desk.request' \
         ORDER BY g.call_id LIMIT 1"
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
