//! Static onboarding and investigation guidance, and the guided setup
//! prompt; all authorized evidence remains in the owning tools.

use rmcp::model::{
    CallToolResult, ErrorData, GetPromptRequestParams, GetPromptResult, Prompt, PromptMessage,
    Role, Tool,
};
use serde_json::{Map, Value, json};
use wyrd_spec::error::WyrdError;
use wyrd_spec::mcp::{
    GuideExample, GuideRequest, GuideResponse, GuideTopic, SETUP_PROMPT_NAME, setup_prompt,
};

use super::principals::{parse_args, tool};
use super::structured;

/// Wire name of authenticated, tenant-independent workflow guidance.
pub(super) const GUIDE: &str = "wyrd.guide";

/// Advertise one closed, typed read-only guide to every authenticated caller.
pub(super) fn descriptor() -> Tool {
    tool::<GuideRequest, GuideResponse>(
        GUIDE,
        "Guide Wyrd onboarding or an investigation",
        "Explain the start onboarding workflow, or the overview, cards, verification, or \
         bifrost_sql investigation workflows, with bounded tool examples. Contains no tenant data and grants no permissions. Replace example \
         placeholders with exact discovered identifiers; consult tools/list for current schemas.",
        true,
    )
}

/// Describe the argument-free `setup` prompt, labelled with this release.
pub(super) fn prompt_descriptor() -> Prompt {
    Prompt::new(
        SETUP_PROMPT_NAME,
        Some(format!(
            "Wyrd {} guided setup: add Wyrd to the current project and confirm one stored verification. Instruction text only; it grants no permission and runs nothing.",
            env!("CARGO_PKG_VERSION")
        )),
        None,
    )
}

/// Return the release's setup prompt as one user message.
///
/// The text is the same [`setup_prompt`] projection `wyrd init` prints for
/// this release. Reading it executes no step and reads no tenant state.
///
/// # Errors
/// Returns invalid parameters for any other prompt name or for arguments,
/// which this prompt does not take.
pub(super) fn prompt(request: &GetPromptRequestParams) -> Result<GetPromptResult, ErrorData> {
    if request.name != SETUP_PROMPT_NAME {
        return Err(ErrorData::invalid_params(
            format!(
                "unknown prompt `{}`; the only prompt is `{SETUP_PROMPT_NAME}`",
                request.name
            ),
            None,
        ));
    }
    if request
        .arguments
        .as_ref()
        .is_some_and(|arguments| !arguments.is_empty())
    {
        return Err(ErrorData::invalid_params(
            format!("the `{SETUP_PROMPT_NAME}` prompt takes no arguments"),
            None,
        ));
    }
    Ok(GetPromptResult::new(vec![PromptMessage::new_text(
        Role::User,
        setup_prompt(env!("CARGO_PKG_VERSION")),
    )])
    .with_description(prompt_descriptor().description.unwrap_or_default()))
}

/// Decode a topic and return bounded static guidance without reading tenant state.
///
/// # Errors
/// Returns invalid-argument or response-serialization errors.
pub(super) fn invoke(arguments: Option<Map<String, Value>>) -> Result<CallToolResult, WyrdError> {
    let request: GuideRequest = parse_args(arguments, GUIDE)?;
    structured(&response(request.topic))
}

/// Build a tool example from a statically authored argument object.
///
/// # Panics
/// Panics if a source example passes a non-object argument value.
fn example(tool: &str, purpose: &str, arguments: Value) -> GuideExample {
    let Value::Object(arguments) = arguments else {
        panic!("guide examples are tool argument objects");
    };
    GuideExample {
        tool: tool.to_owned(),
        purpose: purpose.to_owned(),
        arguments,
    }
}

/// A bounded SQL example uses Oracle's existing result ceilings.
fn sql_example(purpose: &str, sql: &str) -> GuideExample {
    example(
        "bifrost.query",
        purpose,
        json!({
            "sql": sql, "deadline_ms": 10000, "max_rows": 100, "max_bytes": 65536,
        }),
    )
}

/// Explain a closed workflow using exact tool and schema names.
///
/// `start` names CLI commands and SDK operations as text, because MCP neither
/// declares Cards, pulls bundles, uploads files, nor ingests application data;
/// its examples call only shipped read tools.
fn response(topic: GuideTopic) -> GuideResponse {
    let (steps, examples) = match topic {
        GuideTopic::Start => (
            vec![
                "Wyrd verifies an application you own in three steps: Declare its components and Verifiers with the wyrd CLI, Observe it with a Wyrd SDK inside the application, and Verify its behavior through the SDK or this MCP server. MCP teaches, reads Cards and evidence, and runs verification. It does not declare Cards, pull bundles, upload files, or ingest application telemetry or rows; use a shell for those.",
                "Read the docs before writing code. llms.txt is the page index and llms-full.txt the full text, including every embedded YAML and code example, both under the docs base /wyrd/ (published at https://bohmian-ai.github.io/wyrd/; a locally served docs build uses the same paths). Follow get-started/setup/, then get-started/quickstart/, then the build-and-verify Declare, Observe, and Verify pages. Stable error codes and remediation are listed at api/errors/.",
                "Setup: the person installs the wyrd CLI, supplies PostgreSQL, and runs wyrd init in a terminal or background job they own, waiting for readiness; wyrd init --format json prints one readiness line and keeps serving. The MCP prompt setup, and wyrd init --print-prompt, give the guided first-verification steps. Do not start, stop, or restart that server yourself. It saves the endpoint and an administrator credential that the CLI and SDKs resolve with no arguments. wyrd mcp install --host <host> --format json connects a host without writing a credential into its configuration; the host must reload or reconnect afterwards.",
                "Declare: map the application to one Service Card YAML file with components (an alias and a ref) for its Agent, Prompt, and Model Cards, tables with a JSON schema for each kind of custom row, and verified_by bindings to Verifier Cards. A gateway deployment and model string such as openai/gpt-4o is a route, not a Model Card; declare a Model Card only for model artifacts you own, through its existing artifact workflow, before referring to it. Run wyrd declare <dir> --check --format json, which validates the local files and never contacts the server, then wyrd declare <dir> --format json. Repeating an unchanged declaration is idempotent. The receipt's root is the Service's exact identity; declaring returns no secret and grants no authority.",
                "Read and pull: wyrd get --kind Service --name <name> --version <version>, or cards.get here, reads one Card envelope. wyrd pull --kind Service --uid <root_uid> --output-dir .wyrd-bundle, or the SDK's Cards.pull, writes the complete bundle the application loads. A --metadata-only bundle is inspectable but not runnable.",
                "Observe: install the Python SDK (uv add \"wyrd-ai[otel]\", plus agents or gateway for those integrations), the Rust wyrd-ai crate, or the TypeScript wyrd-ai package. Load the bundle with WyrdState.from_path, call start_bifrost and start_telemetry, open state.run() for each unit of work, write rows with run.observe.record(<table>, row) matching the declared schema, use run.for_card(<alias>) for a component's view, and call state.shutdown() before exit to flush buffered rows and spans. Recording buffers rows; a row becomes queryable only after the server publishes it.",
                "Verify: run.observe.verify(<verifier>, <slot>=value) returns a judgment within the server deadline; a failed verdict is a result, not an error, and dispatches no Operator. run.observe.submit(...) queues the work and returns an execution; poll it from any process with verify.Execution.connect(execution_id).status() or verification.get_execution until it is completed, errored, or timed_out. Here, verification.execute and verification.enqueue take the same inline inputs and require verifier:run on the Verifier. A path on your machine is not server-readable; send file evidence from the SDK, such as verify.File. Drift Verifiers run only on their binding schedule.",
                "Done means server evidence, not your own output: read the Service and its Verifier bindings with cards.get, query your custom table and bifrost.traces.spans by run_id with bifrost.query, and read each judgment from bifrost.verification.results. Use the cards, verification, and bifrost_sql topics for those reads. Report refusals, empty results, and failed queries as they are.",
            ],
            vec![
                example(
                    "cards.list",
                    "Find the declared Service's exact versions",
                    json!({ "kind": "Service", "name": "<service_name>", "limit": 20 }),
                ),
                example(
                    "verification.get_execution",
                    "Poll a submitted verification to a terminal status",
                    json!({ "execution_id": "<execution_id>" }),
                ),
                example(
                    GUIDE,
                    "Learn evidence queries",
                    json!({ "topic": "bifrost_sql" }),
                ),
            ],
        ),
        GuideTopic::Overview => (
            vec![
                "Use tools/list to inspect the current caller-dependent catalog and each tool's input/output schema. Discovery does not grant permission; a named call can still be refused.",
                "For a Service name, start with cards.list. For a request or trace, discover and describe Bifrost tables. For a Verifier execution, read verification.get_execution. Choose the investigation path from the evidence.",
                "Use wyrd.guide topic start to declare, observe, and verify an application, and topics cards, verification, and bifrost_sql for those investigations. Consult tool descriptors for principal credentials, Operator connections, and gateway administration.",
                "Cite exact Card UIDs and versions, result IDs, table names, and returned rows. State empty evidence, uncertainty, refusals, and failed queries explicitly.",
            ],
            vec![example(
                GUIDE,
                "Learn bounded SQL",
                json!({ "topic": "bifrost_sql" }),
            )],
        ),
        GuideTopic::Cards => (
            vec![
                "Call cards.list with kind/name or other advertised filters. Read metadata summaries, select an explicit version, and follow next_cursor unchanged to continue the same filtered list. There is no fuzzy search or implicit newest version.",
                "Call cards.get with the selected card_ref, including its version, or with kind and card_uid; pass exactly one form. It requires cards:read and returns the full Card.",
                "Use Service spec.components and declared tables to establish scope. card.relationships.outbound_refs and inbound_refs contain exact related identities in each edge's ref field; follow them with repeated cards.get calls by card_ref.",
                "A principal is an authenticated identity and a Card is a versioned component. Do not substitute principal IDs for Card UIDs. Missing and foreign-tenant Cards are indistinguishable; stop or narrow the question on refusal.",
            ],
            vec![
                example(
                    "cards.list",
                    "Find exact Service versions",
                    json!({ "kind": "Service", "name": "<service_name>", "limit": 20 }),
                ),
                example(
                    "cards.get",
                    "Read the selected version or relationship",
                    json!({ "card_ref": { "kind": "Service", "name": "<service_name>", "version": "1.0.0" } }),
                ),
            ],
        ),
        GuideTopic::Verification => (
            vec![
                "Read the owner Card's status.verification.binding_ids and call verification.get_binding for the exact owner, subject, Verifier, readiness, and last_execution_id.",
                "Call verification.get_execution for execution status, its result, execution errors, and Operator dispatches. A failed judgment differs from an execution failure. Pending, errored, and timed-out executions do not establish a completed judgment. A just-enqueued execution may read as not found until the queue accepts it.",
                "Persisted judgment history lives in Bifrost SQL: query bifrost.verification.results by result_id, or by exact subject_card_uid or workload_card_uid within a suitable time window. Then inspect bifrost.drift.result_features by matching result_id.",
                "subject_card_uid identifies the Card verified, workload_card_uid the owner Service, and verifier_card_uid the Verifier; sub_principal_id and act_principal_id record who the work was for and who performed it. Verification details follow table query permission. Gateway request/response payload columns additionally require gateway_payload:read. Empty evidence is not proof of a passing judgment.",
                "verification.execute and verification.enqueue require verifier:run on the Verifier and take the same inline inputs. verification.execute returns the judgment directly and dispatches no Operator. verification.enqueue accepts the work with 202 and an execution_id; poll verification.get_execution. File inputs are referenced by digest: verification.evidence plans their upload, and the SDK or HTTP client sends the bytes; a local path is not server-readable. Drift Verifiers judge windows and run only on their binding schedule.",
            ],
            vec![
                example(
                    "verification.get_binding",
                    "Read a discovered binding",
                    json!({ "binding_id": "<binding_id>" }),
                ),
                example(
                    "verification.get_execution",
                    "Read an execution's status and result",
                    json!({ "execution_id": "<execution_id>" }),
                ),
                sql_example(
                    "Find recent subject judgments",
                    "SELECT result_id, verifier_card_uid, verdict, subject_card_uid FROM bifrost.verification.results WHERE subject_card_uid = '<subject_card_uid>' AND ended_at >= TIMESTAMP '2026-01-01 00:00:00' AND ended_at < TIMESTAMP '2027-01-01 00:00:00' ORDER BY ended_at DESC LIMIT 20",
                ),
            ],
        ),
        GuideTopic::BifrostSql => (
            vec![
                "Call bifrost.list_tables, then bifrost.describe_table for every table used. Inspect field names and types, including correlation fields. Qualify table names exactly as namespace.name from discovery; do not guess a custom table's fields.",
                "Use a single DataFusion SELECT with explicit projections, suitable time bounds, and LIMIT. bifrost.query accepts sql and optional params (ordered scalar binds for $1..$n), deadline_ms, max_rows, max_bytes. Replace placeholders with discovered identifiers and adjust the example time windows.",
                "Inspect columns once, map positional rows to their column order, and inspect terminal. Treat isError or a failed terminal as failure; do not use incomplete rows as a complete answer. Result ceilings refuse oversized results rather than truncating them.",
                "Correlate telemetry using described trace_id/span_id and time columns. For verification, use exact subject/owner Card UIDs, then result_id to inspect Drift details. Use only tables and payload columns the caller is authorized to read.",
                "The custom-table example assumes discovery describes id and value. A Service declares custom tables in bifrost.datasets; replace the qualified name and projection with the actual declared schema.",
            ],
            vec![
                example(
                    "bifrost.list_tables",
                    "Discover authorized table names",
                    json!({}),
                ),
                example(
                    "bifrost.describe_table",
                    "Read telemetry fields and types",
                    json!({ "namespace": "bifrost.traces", "name": "spans" }),
                ),
                sql_example(
                    "Correlate an error log to its trace span",
                    "SELECT l.severity_text, l.event_name, s.name, s.status_code FROM bifrost.logs.records l JOIN bifrost.traces.spans s ON l.trace_id = s.trace_id AND l.span_id = s.span_id WHERE s.trace_id = X'<trace_id>' AND s.start_time_unix_nano >= TIMESTAMP '2026-01-01T00:00:00Z' AND s.start_time_unix_nano < TIMESTAMP '2027-01-01T00:00:00Z' LIMIT 20",
                ),
                sql_example(
                    "Read a completed judgment",
                    "SELECT result_id, execution_id, execution_status, verdict, subject_card_uid FROM bifrost.verification.results WHERE result_id = '<result_id>' LIMIT 20",
                ),
                sql_example(
                    "Read scored Drift features",
                    "SELECT result_id, feature, score, threshold, verdict FROM bifrost.drift.result_features WHERE result_id = '<result_id>' LIMIT 20",
                ),
                sql_example(
                    "Read a described custom table",
                    "SELECT id, value FROM <custom_table> WHERE id >= 1 ORDER BY id LIMIT 20",
                ),
            ],
        ),
    };
    GuideResponse {
        topic,
        steps: steps.into_iter().map(str::to_owned).collect(),
        examples,
    }
}
