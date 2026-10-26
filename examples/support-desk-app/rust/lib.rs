//! A customer-support API whose answers are checked by Wyrd.
//!
//! [`SupportDesk::deploy`] declares the Cards in `service/` and loads them
//! back as a [`WyrdState`]. [`SupportDesk::serve`] serves the three requests
//! of [`SupportDesk::router`] with Wyrd telemetry, then flushes on exit:
//!
//! - `POST /ask` answers a question and runs the two model-free Verifiers
//!   now with `verify`: the Service's `no-refund-promise` and the agent's
//!   `ticket-complete`;
//! - `POST /reviews` has the agent write a refund decision letter and
//!   `submit`s it to the agent's model-judged `follows-policy` Verifier,
//!   returning its execution ID;
//! - `GET /reviews/{id}` reads that check back by its ID alone.
//!
//! Every answer renders the `support-agent` Card's Prompt and calls its model
//! through the Wyrd gateway with the Run's `wyrd-run-id` and `wyrd-card-uid`
//! headers, so the Run's ticket, request span, gateway call, and verdicts
//! share its `run_id`.
//! A Verifier is called on the view of the Card it is bound to: the Run
//! itself for the Service, and `run.for_card("agent")` for the agent. Both
//! views share one `run_id`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::Instrument as _;
use wyrd_ai::bifrost::{BifrostOptions, QueryParam};
use wyrd_ai::cards::{BundleMode, CardSelector, Cards};
use wyrd_ai::gateway_admin::{GatewayCaptureMode, GatewayCapturePolicyWrite};
use wyrd_ai::observe::Run;
use wyrd_ai::otel::start_telemetry;
use wyrd_ai::state::WyrdState;
use wyrd_ai::verify::{Execution, File, Input, Inputs, Submission};
use wyrd_ai::{
    Bifrost, ExecutionId, ExecutionRoute, Gateway, RequirementResult, VerificationResult,
    VerificationVerdict, WyrdClient, WyrdError,
};

/// The `service/` directory holding this app's Cards.
#[must_use]
pub fn service_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("service")
}

/// The deployed support desk: the hydrated Service every request runs in.
#[derive(Clone)]
pub struct SupportDesk {
    /// The Service's Cards, loaded from the Card bundle.
    state: WyrdState,
}

impl SupportDesk {
    /// Declare `service/support-desk.yaml` and every Card it references,
    /// pull them into `bundle`, turn on gateway call metadata capture so
    /// each model call is retained as evidence, and start the Bifrost writer
    /// the ticket rows go through.
    ///
    /// Declaring is idempotent, so restarting the app re-declares nothing.
    ///
    /// # Errors
    /// Returns the server's refusal of the declaration, pull, or
    /// capture policy, or the local error loading the bundle.
    pub async fn deploy(client: &WyrdClient, bundle: &Path) -> Result<Self, WyrdError> {
        // ANCHOR: deploy
        let cards = Cards::connect_with(client).await?;
        let receipt = cards
            .declare_from_path(service_dir().join("support-desk.yaml"))
            .await?;
        cards
            .pull(
                CardSelector::exact(receipt.root),
                bundle,
                BundleMode::Complete,
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
        // ANCHOR: init
        let state = WyrdState::from_path_with(bundle, client.clone())?;
        // Starts the buffered writer the ticket rows go through.
        state.start_bifrost().await?;
        // ANCHOR_END: init
        Ok(Self { state })
    }

    /// Serve the routes on `listener` until `stop` resolves, exporting spans
    /// to Wyrd, then flush the spans and drain the buffered tickets.
    ///
    /// [`start_telemetry`] installs the process's `tracing` pipeline and
    /// returns its [`Telemetry`](wyrd_ai::otel::Telemetry) handle, which this call owns for the life of
    /// the server. After serving ends, the handle's shutdown flushes the
    /// spans and the state's shutdown drains the Bifrost writer, in that
    /// order and whether or not serving failed. It must run on a
    /// multi-threaded Tokio runtime, because the span flush blocks while the
    /// export runs on that runtime.
    ///
    /// # Errors
    /// Returns `WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS` when the application
    /// already installed a `tracing` subscriber, the telemetry flush or
    /// writer drain failure, or `WYRD_SPEC_500_INTERNAL` naming the serving
    /// failure. Ticket rows still buffered when a drain fails can be lost.
    pub async fn serve(
        self,
        listener: tokio::net::TcpListener,
        stop: impl Future<Output = ()> + Send + 'static,
    ) -> Result<(), WyrdError> {
        // ANCHOR: serve
        let telemetry = start_telemetry(&self.state)?;
        let state = self.state.clone();
        let served = axum::serve(listener, self.router())
            .with_graceful_shutdown(stop)
            .await;
        // Flush spans first, then drain buffered tickets, before exiting.
        telemetry.shutdown()?;
        state.shutdown().await?;
        // ANCHOR_END: serve
        served.map_err(|error| WyrdError::Internal {
            message: format!("the support API stopped serving: {error}"),
            details: json!({}),
        })
    }

    /// The HTTP routes of the app.
    pub fn router(self) -> Router {
        Router::new()
            .route("/ask", post(ask))
            .route("/reviews", post(review))
            .route("/reviews/{id}", get(review_status))
            .with_state(self)
    }

    /// Answer `question` as `run`'s Agent through the Wyrd gateway and record
    /// the ticket in `bifrost.datasets.tickets`.
    ///
    /// Renders the Agent's Prompt, sends it to `<deployment>/<model>` on the
    /// OpenAI-compatible gateway ingress with a fresh access token, and tags
    /// the call with the Run and Agent Card. The whole exchange, through
    /// reading the reply body, is bounded by [`ANSWER_TIMEOUT`].
    ///
    /// # Errors
    /// Returns the gateway's refusal as [`ApiError::refusal`] projects it, the
    /// state, token, or Bifrost writer's Wyrd error, or a 500 with fixed text
    /// for a Prompt render, transport, timeout, or reply decoding failure, an
    /// Agent that does not route through the gateway, or a reply without
    /// text.
    ///
    /// # Cancellation
    /// The model call and the ticket write are separate steps. A dropped
    /// future or a transport error after sending leaves the call's outcome
    /// unknown, and a call that completed may have no ticket. Nothing retries,
    /// deduplicates, or rolls back either step.
    async fn answer(&self, run: &Run, question: &str) -> Result<String, ApiError> {
        // ANCHOR: answer
        let agent = self.state.agent(run.alias())?;
        let ExecutionRoute::WyrdGateway { deployment } = &agent.card().spec.execution_route else {
            return Err(ApiError::internal(
                "the Agent does not route through the Wyrd gateway",
            ));
        };
        let prompt = &agent.prompt().metadata.prompt;
        let rendered = prompt
            .render(&[("question", question)])
            .map_err(|_| ApiError::internal("the Agent's Prompt does not render"))?;
        let mut body = serde_json::to_value(rendered)
            .map_err(|_| ApiError::internal("the rendered Prompt does not serialize"))?["body"]
            .take();
        body["model"] = json!(format!("{deployment}/{}", prompt.model));
        let client = self.state.client()?;
        // These headers tie the gateway's record of the call to this Run.
        let reply = reqwest::Client::new()
            .post(format!("{}/v1/chat/completions", client.server_url()))
            .bearer_auth(client.access_token().await?.expose())
            .header("wyrd-run-id", run.run_id().as_str())
            .header("wyrd-card-uid", run.subject_uid().as_str())
            .json(&body)
            .timeout(ANSWER_TIMEOUT)
            .send()
            .await
            .map_err(|_| ApiError::internal("the Wyrd gateway call failed"))?;
        let status = reply.status();
        let reply: Option<Value> = reply.json().await.ok();
        if !status.is_success() {
            return Err(ApiError::refusal(status, reply.as_ref()));
        }
        let reply =
            reply.ok_or_else(|| ApiError::internal("the gateway reply could not be read"))?;
        let answer = reply["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| ApiError::internal("the gateway reply carries no text"))?
            .to_owned();
        run.observe()
            .record(
                "bifrost.datasets.tickets",
                &json!({ "question": question, "answer": answer }),
            )
            .await?;
        // ANCHOR_END: answer
        Ok(answer)
    }
}

/// The total budget of one gateway model call, from connecting through the
/// last byte of the reply.
const ANSWER_TIMEOUT: Duration = Duration::from_mins(10);

/// The SQL [`evidence`] runs.
/// Every table it joins carries the Run's `run_id`.
const EVIDENCE: &str = "\
    SELECT t.run_id, t.question, t.answer, s.trace_id, s.name AS span, \
           g.call_id, r.verdict \
    FROM bifrost.datasets.tickets t \
    JOIN bifrost.traces.spans s ON s.run_id = t.run_id \
    JOIN bifrost.gateway.calls g ON g.run_id = t.run_id \
    JOIN bifrost.verification.results r ON r.run_id = t.run_id \
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
/// Returns the [`SupportDesk::answer`] error or a check's Wyrd error.
///
/// # Cancellation
/// The steps run in order and nothing is rolled back. A failure or a dropped
/// request after the model call keeps that call and any ticket or verdict
/// already recorded, and returns no answer.
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
        Ok::<_, ApiError>((answer, promise, complete))
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
/// Returns the [`SupportDesk::answer`] error, the submission's Wyrd error, or
/// a 500 with fixed text when the check is recorded rather than queued.
///
/// # Cancellation
/// The steps run in order and nothing is rolled back. A failure or a dropped
/// request after the model call keeps that call and its ticket. A submission
/// whose reply is lost may still be queued, and the caller never learns its
/// execution ID.
async fn review(
    State(desk): State<SupportDesk>,
    Json(body): Json<Question>,
) -> Result<Json<Value>, ApiError> {
    let run = desk.state.run_for_card("agent")?;
    let work = async {
        let letter = desk
            .answer(&run, &format!("{LETTER}{}", body.question))
            .await?;
        // ANCHOR: submit
        let submission = run
            .observe()
            .submit("follows-policy", letter_inputs(&letter))
            .await?;
        // ANCHOR_END: submit
        Ok::<_, ApiError>((letter, submission))
    };
    let (letter, submission) = run
        .scope(work.instrument(tracing::info_span!("support-desk.review")))
        .await?;
    // A Verifier that judges Run evidence is always queued; only a
    // window-scoped one is merely recorded.
    let Submission::Queued(execution) = submission else {
        return Err(ApiError::internal(
            "follows-policy was recorded, not queued",
        ));
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

/// An HTTP error response carrying a status and, for a catalogued Wyrd
/// error, its stable code and message.
#[derive(Debug)]
pub struct ApiError {
    /// The HTTP status.
    status: StatusCode,
    /// The JSON body.
    body: Value,
}

impl ApiError {
    /// A 500 carrying the fixed `message`, for a failure outside the Wyrd
    /// error catalog. The message is static so no transport, render, or
    /// decoder detail reaches the response.
    fn internal(message: &'static str) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            body: json!({ "message": message }),
        }
    }

    /// Project a gateway refusal with `status` and decoded `reply` body.
    ///
    /// The status is kept. An `error.code` in the Wyrd catalog is kept, with
    /// the catalog's own title as the message when the code names a variant
    /// [`WyrdError::from_code`] rebuilds and fixed text naming the status
    /// otherwise. The reply's own message and every provider field are
    /// dropped, because the gateway forwards provider error content it did
    /// not author.
    fn refusal(status: StatusCode, reply: Option<&Value>) -> Self {
        let fixed = format!("the Wyrd gateway answered HTTP {}", status.as_u16());
        let code = reply
            .and_then(|reply| reply["error"]["code"].as_str())
            .and_then(|code| WyrdError::codes().into_iter().find(|known| *known == code));
        let body = match code {
            Some(code) => json!({
                "code": code,
                "message": WyrdError::from_code(code, String::new(), json!({}))
                    .map_or(fixed, |error| error.title().to_owned()),
            }),
            None => json!({ "message": fixed }),
        };
        Self { status, body }
    }
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
