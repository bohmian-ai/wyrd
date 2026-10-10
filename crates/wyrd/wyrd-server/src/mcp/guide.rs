//! Static investigation guidance; all authorized evidence remains in the owning tools.

use rmcp::model::{CallToolResult, Tool};
use serde_json::{Map, Value, json};
use wyrd_spec::error::WyrdError;
use wyrd_spec::mcp::{GuideExample, GuideRequest, GuideResponse, GuideTopic};

use super::principals::{parse_args, tool};
use super::structured;

/// Wire name of authenticated, tenant-independent workflow guidance.
pub(super) const GUIDE: &str = "wyrd.guide";

/// Advertise one closed, typed read-only guide to every authenticated caller.
pub(super) fn descriptor() -> Tool {
    tool::<GuideRequest, GuideResponse>(
        GUIDE,
        "Guide a Wyrd investigation",
        "Explain overview, cards, verification, or bifrost_sql workflows with bounded tool \
         examples. Contains no tenant data and grants no permissions. Replace example \
         placeholders with exact discovered identifiers; consult tools/list for current schemas.",
        true,
    )
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
fn response(topic: GuideTopic) -> GuideResponse {
    let (steps, examples) = match topic {
        GuideTopic::Overview => (
            vec![
                "Use tools/list to inspect the current caller-dependent catalog and each tool's input/output schema. Discovery does not grant permission; a named call can still be refused.",
                "For a Service name, start with cards.list. For a request or trace, discover and describe Bifrost tables. For a run, read verification.get_run. Choose the investigation path from the evidence.",
                "Use wyrd.guide topics cards, verification, and bifrost_sql for those workflows. Consult tool descriptors for principal credentials, Operator connections, and gateway administration.",
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
                "Call cards.list with kind/name/space or other advertised filters. Read metadata summaries, select an explicit version, and follow next_cursor unchanged to continue the same filtered list. There is no fuzzy search or implicit newest version.",
                "Call cards.get_by_ref with the selected CardRef, including version; cards.get remains the kind-qualified UID read. Both require cards:read and return the full Card.",
                "Use Service spec.components and declared tables to establish scope. card.relationships.outbound_refs and inbound_refs contain exact related identities in each edge's ref field; follow them with repeated cards.get_by_ref calls.",
                "A principal is an authenticated identity and a Card is a versioned component. Do not substitute principal IDs for Card UIDs. Missing and foreign-tenant Cards are indistinguishable; stop or narrow the question on refusal.",
            ],
            vec![
                example(
                    "cards.list",
                    "Find exact Service versions",
                    json!({ "kind": "Service", "name": "<service_name>", "limit": 20 }),
                ),
                example(
                    "cards.get_by_ref",
                    "Read the selected version or relationship",
                    json!({ "card_ref": { "kind": "Service", "name": "<service_name>", "version": "1.0.0", "space": "default" } }),
                ),
            ],
        ),
        GuideTopic::Verification => (
            vec![
                "Read the owner Card's status.verification.binding_ids and call verification.get_binding for the exact owner, subject, Verifier, readiness, and last run.",
                "Call verification.get_run for execution status, result_id, execution errors, and Operator dispatches. A failed judgment differs from an execution failure. Pending, errored, and timed-out runs do not establish a completed judgment.",
                "Persisted judgment history lives in Bifrost SQL: query vala.verification.results by result_id, or by exact subject_card_uid or owner_card_uid within a suitable time window. Then inspect vala.drift.result_features or vala.eval.result_items by matching result_id.",
                "Managed card_uid identifies the Verifier; subject_card_uid identifies the component judged. Verification details and Eval actual/expected/message follow table query permission. Gateway request/response payload columns additionally require gateway_payload:read. Empty evidence is not proof of a passing judgment.",
                "verification.execute requires verifier:run and returns a one-off judgment directly. It enqueues, persists, and dispatches nothing. verification.start_run durably enqueues a bounded Drift window; poll its run and read published evidence through SQL.",
            ],
            vec![
                example(
                    "verification.get_binding",
                    "Read a discovered binding",
                    json!({ "binding_id": "<binding_id>" }),
                ),
                example(
                    "verification.get_run",
                    "Find the run's result ID",
                    json!({ "run_id": "<run_id>" }),
                ),
                sql_example(
                    "Find recent subject judgments",
                    "SELECT result_id, implementation, verdict, subject_card_uid FROM vala.verification.results WHERE subject_card_uid = '<subject_card_uid>' AND ended_at >= TIMESTAMP '2026-01-01 00:00:00' AND ended_at < TIMESTAMP '2027-01-01 00:00:00' ORDER BY ended_at DESC LIMIT 20",
                ),
            ],
        ),
        GuideTopic::BifrostSql => (
            vec![
                "Call bifrost.list_tables, then bifrost.describe_table for every table used. Inspect field names and types, including correlation fields. Qualify table names exactly as namespace.name from discovery; do not guess a custom table's fields.",
                "Use a single DataFusion SELECT with explicit projections, suitable time bounds, and LIMIT. bifrost.query accepts sql and optional deadline_ms, max_rows, max_bytes. Replace placeholders with discovered identifiers and adjust the example time windows.",
                "Inspect columns once, map positional rows to their column order, and inspect terminal. Treat isError or a failed terminal as failure; do not use incomplete rows as a complete answer. Result ceilings refuse oversized results rather than truncating them.",
                "Correlate telemetry using described trace_id/span_id and time columns. For verification, use exact subject/owner Card UIDs, then result_id to inspect Drift/Eval details. Use only tables and payload columns the caller is authorized to read.",
                "The custom-table example assumes discovery describes id and value. A Service declares custom tables in vala.datasets; replace the qualified name and projection with the actual declared schema.",
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
                    json!({ "namespace": "vala.traces", "name": "spans" }),
                ),
                sql_example(
                    "Correlate an error log to its trace span",
                    "SELECT l.severity_text, l.event_name, s.name, s.status_code FROM vala.logs.records l JOIN vala.traces.spans s ON l.trace_id = s.trace_id AND l.span_id = s.span_id WHERE s.trace_id = X'<trace_id>' AND s.start_time_unix_nano >= 1767225600000000000 AND s.start_time_unix_nano < 1798761600000000000 LIMIT 20",
                ),
                sql_example(
                    "Read a completed judgment",
                    "SELECT result_id, implementation, execution_status, verdict, subject_card_uid FROM vala.verification.results WHERE result_id = '<result_id>' LIMIT 20",
                ),
                sql_example(
                    "Read scored Drift features",
                    "SELECT result_id, feature, score, threshold, verdict FROM vala.drift.result_features WHERE result_id = '<result_id>' LIMIT 20",
                ),
                sql_example(
                    "Read Eval outcomes without sensitive payloads",
                    "SELECT result_id, task_id, outcome_kind, passed, skip_reason FROM vala.eval.result_items WHERE result_id = '<result_id>' LIMIT 20",
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
