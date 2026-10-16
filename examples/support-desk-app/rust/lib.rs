//! A customer-support API whose answers are checked by Wyrd.
//!
//! [`SupportDesk::deploy`] registers the Cards in `service/` and loads them
//! back as a [`WyrdState`]. [`SupportDesk::router`] serves three requests:
//!
//! - `POST /ask` answers a question and runs the two model-free Verifiers
//!   now with `verify`: the Service's `no-refund-promise` and the agent's
//!   `ticket-complete`;
//! - `POST /reviews` has the agent write a refund decision letter and
//!   `submit`s it to the agent's model-judged `follows-policy` Verifier,
//!   returning its execution ID;
//! - `GET /reviews/{id}` reads that check back by its ID alone.
//!
//! Every answer comes from the `support-agent` Card through `Run::invoke`, so
//! the model call goes through the Wyrd gateway inside a Wyrd Run, and the
//! Run's ticket, request span, gateway call, and verdicts share its `run_id`.
//! A Verifier is called on the view of the Card it is bound to: the Run
//! itself for the Service, and `run.for_card("agent")` for the agent. Both
//! views share one `run_id`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::Instrument as _;
use wyrd_ai::bifrost::{BifrostOptions, QueryParam};
use wyrd_ai::cards::{CardSelector, Cards, HydrationMode};
use wyrd_ai::gateway::{GatewayCaptureMode, GatewayCapturePolicyWrite};
use wyrd_ai::observe::Run;
use wyrd_ai::state::WyrdState;
use wyrd_ai::verify::{Execution, File, Input, Inputs, Submission};
use wyrd_ai::{
    Bifrost, ExecutionId, Gateway, RequirementResult, VerificationResult, VerificationVerdict,
    WyrdClient, WyrdError,
};

/// The `service/` directory holding this app's Cards.
#[must_use]
pub fn service_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("service")
}

/// The deployed support desk: the hydrated Service every request runs in.
#[derive(Clone)]
pub struct SupportDesk {
    /// The Service's Cards, loaded from the hydrated bundle.
    state: WyrdState,
}

impl SupportDesk {
    /// Register `service/support-desk.yaml` and every Card it references,
    /// hydrate them into `bundle`, turn on gateway call metadata capture so
    /// each model call is retained as evidence, and start the Bifrost writer
    /// the ticket rows go through.
    ///
    /// Registering is idempotent, so restarting the app re-registers nothing.
    ///
    /// # Errors
    /// Returns the server's refusal of the registration, hydration, or
    /// capture policy, or the local error loading the bundle.
    pub async fn deploy(client: &WyrdClient, bundle: &Path) -> Result<Self, WyrdError> {
        // ANCHOR: deploy
        let cards = Cards::connect_with(client).await?;
        let receipt = cards
            .register_from_path(service_dir().join("support-desk.yaml"))
            .await?;
        cards
            .hydrate(
                CardSelector::exact(receipt.root),
                bundle,
                HydrationMode::Complete,
            )
            .await?;
        Gateway::connect_with(client)
            .await?
            .put_capture_policy(&GatewayCapturePolicyWrite {
                mode: GatewayCaptureMode::Metadata,
                payload_fields: BTreeSet::new(),
            })
            .await?;
        // ANCHOR_END: deploy
        let state = WyrdState::from_path_with(bundle, client.clone())?;
        state.start_bifrost().await?;
        Ok(Self { state })
    }

    /// The hydrated Service, for telemetry setup and shutdown.
    #[must_use]
    pub fn state(&self) -> &WyrdState {
        &self.state
    }

    /// The HTTP routes of the app.
    pub fn router(self) -> Router {
        Router::new()
            .route("/ask", post(ask))
            .route("/reviews", post(review))
            .route("/reviews/{id}", get(review_status))
            .with_state(self)
    }

    /// Answer `question` in `run` through the Wyrd gateway and record the
    /// ticket in `vala.datasets.tickets`.
    ///
    /// # Errors
    /// Returns the gateway's or the Bifrost writer's error.
    async fn answer(&self, run: &Run, question: &str) -> Result<String, WyrdError> {
        // ANCHOR: answer
        let answer = run.invoke(&[("question", question)]).await?;
        run.observe()
            .record(
                "vala.datasets.tickets",
                &json!({ "question": question, "answer": answer }),
            )
            .await?;
        // ANCHOR_END: answer
        Ok(answer)
    }
}

/// The SQL [`evidence`] runs.
/// Every table it joins carries the Run's `run_id`.
const EVIDENCE: &str = "\
    SELECT t.run_id, t.question, t.answer, s.trace_id, s.name AS span, \
           g.call_id, r.verdict \
    FROM vala.datasets.tickets t \
    JOIN vala.traces.spans s ON s.run_id = t.run_id \
    JOIN vala.gateway.calls g ON g.run_id = t.run_id \
    JOIN vala.verification.results r ON r.run_id = t.run_id \
    WHERE t.run_id = $1 AND s.name LIKE 'support-desk.%'";

/// One joined evidence row.
#[derive(Debug, Deserialize)]
pub struct Evidence {
    /// The Run that recorded the ticket.
    pub run_id: String,
    /// The customer's question.
    pub question: String,
    /// The Agent's answer.
    pub answer: String,
    /// The request span's trace.
    pub trace_id: String,
    /// The request span's name.
    pub span: String,
    /// The gateway's ID of the model call.
    pub call_id: String,
    /// One Verifier's verdict; a Run has a row per verdict.
    pub verdict: String,
}

/// Read every joined evidence row of Run `run_id` as `client`.
///
/// Rows land in Bifrost shortly after the request, so a Run read right away
/// can return nothing yet.
///
/// # Errors
/// Returns the Bifrost connection or query error.
pub async fn evidence(client: &WyrdClient, run_id: &str) -> Result<Vec<Evidence>, WyrdError> {
    Ok(Bifrost::connect_with(client, BifrostOptions::default())
        .await?
        .sql_as(EVIDENCE, &[QueryParam::String(run_id.to_owned())])
        .await?)
}

/// The `follows-policy` input: the agent's Markdown letter, uploaded as a file.
fn letter_inputs(letter: &str) -> Inputs {
    Inputs::named([(
        "letter",
        Input::from(File::bytes(letter.to_owned()).with_filename("letter.md")),
    )])
}

/// What `POST /reviews` asks the agent for: a document, not a chat reply.
const LETTER: &str = "Write the customer a refund decision letter in Markdown. Their request: ";

/// The body of `POST /ask` and `POST /reviews`.
#[derive(Deserialize)]
pub struct Question {
    /// The customer's question.
    pub question: String,
    /// The order the question is about, when the customer gave one.
    pub order_id: Option<String>,
}

/// A check's verdict and the outcome of each of its requirements.
#[derive(Serialize)]
struct Breakdown<'a> {
    /// `passed`, `failed`, or `inconclusive`.
    verdict: VerificationVerdict,
    /// Each requirement's outcome, reason, and policy citations.
    requirements: &'a [RequirementResult],
}

impl<'a> Breakdown<'a> {
    /// The breakdown of `result`.
    fn of(result: &'a VerificationResult) -> Self {
        Self {
            verdict: result.outcome.verdict,
            requirements: &result.outcome.requirements,
        }
    }
}

/// `POST /ask`: answer the question in a new Run of the Service and run the
/// two model-free Verifiers now.
///
/// `no-refund-promise` is bound to the Service, so it runs on the Run itself.
/// `ticket-complete` is bound to the agent, so it runs on the Run's agent
/// view, which shares the `run_id`.
///
/// # Errors
/// Returns the Wyrd error of the model call, the ticket write, or a check.
async fn ask(
    State(desk): State<SupportDesk>,
    Json(body): Json<Question>,
) -> Result<Json<Value>, ApiError> {
    // ANCHOR: check
    let run = desk.state.run();
    let agent = run.for_card("agent")?;
    let work = async {
        let answer = desk.answer(&agent, &body.question).await?;
        let promise = run
            .observe()
            .verify(
                "no-refund-promise",
                Inputs::named([("answer", Input::from(json!(answer)))]),
            )
            .await?;
        let ticket = json!({
            "question": body.question,
            "answer": answer,
            "order_id": body.order_id,
        });
        let complete = agent
            .observe()
            .verify(
                "ticket-complete",
                Inputs::named([("ticket", Input::from(ticket))]),
            )
            .await?;
        Ok::<_, WyrdError>((answer, promise, complete))
    };
    let (answer, promise, complete) = run
        .scope(work.instrument(tracing::info_span!("support-desk.ask")))
        .await?;
    // ANCHOR_END: check
    Ok(Json(json!({
        "run_id": run.run_id().to_string(),
        "answer": answer,
        "checks": {
            "no-refund-promise": Breakdown::of(&promise),
            "ticket-complete": Breakdown::of(&complete),
        },
    })))
}

/// `POST /reviews`: have the agent write a refund decision letter in a new Run
/// and submit it to `follows-policy`, returning the check's execution ID
/// without waiting.
///
/// # Errors
/// Returns the Wyrd error of the model call, the ticket write, or the
/// submission.
async fn review(
    State(desk): State<SupportDesk>,
    Json(body): Json<Question>,
) -> Result<Json<Value>, ApiError> {
    let run = desk.state.run_for_card("agent")?;
    let work = async {
        let letter = desk.answer(&run, &format!("{LETTER}{}", body.question)).await?;
        // ANCHOR: submit
        let submission = run
            .observe()
            .submit("follows-policy", letter_inputs(&letter))
            .await?;
        // ANCHOR_END: submit
        Ok::<_, WyrdError>((letter, submission))
    };
    let (letter, submission) = run
        .scope(work.instrument(tracing::info_span!("support-desk.review")))
        .await?;
    // A Verifier that judges Run evidence is always queued; only a
    // window-scoped one is merely recorded.
    let Submission::Queued(execution) = submission else {
        return Err(ApiError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            body: json!({ "message": "follows-policy was recorded, not queued" }),
        });
    };
    Ok(Json(json!({
        "run_id": run.run_id().to_string(),
        "letter": letter,
        "execution_id": execution.id().to_string(),
    })))
}

/// `GET /reviews/{id}`: the check's status and, once it completed, its
/// breakdown. Only the ID is needed, so any process can read it.
///
/// # Errors
/// Returns 400 for a malformed ID and the server's error for an unknown one.
async fn review_status(
    State(desk): State<SupportDesk>,
    UrlPath(id): UrlPath<String>,
) -> Result<Json<Value>, ApiError> {
    let id: ExecutionId = id.parse().map_err(|_| ApiError {
        status: StatusCode::BAD_REQUEST,
        body: json!({ "message": format!("{id} is not an execution ID") }),
    })?;
    // ANCHOR: status
    let state = Execution::connect_with(id, desk.state.client()?)
        .await?
        .status()
        .await?;
    // ANCHOR_END: status
    Ok(Json(json!({
        "status": state.status,
        "check": state.result.as_ref().map(Breakdown::of),
        "error": state.error,
    })))
}

/// An HTTP error response carrying the Wyrd error's status, code, and
/// message.
#[derive(Debug)]
pub struct ApiError {
    /// The HTTP status.
    status: StatusCode,
    /// The JSON body.
    body: Value,
}

impl From<WyrdError> for ApiError {
    /// Keep the Wyrd error's own HTTP status and stable code.
    fn from(error: WyrdError) -> Self {
        Self {
            status: StatusCode::from_u16(error.status())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            body: json!({ "code": error.code(), "message": error.to_string() }),
        }
    }
}

impl IntoResponse for ApiError {
    /// Render the status and JSON body.
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}
