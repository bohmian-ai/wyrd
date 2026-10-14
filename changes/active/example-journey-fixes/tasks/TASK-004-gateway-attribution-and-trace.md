# TASK-004 Judge call attribution and trace linking

Spec: `../spec.md` REQ-004, REQ-005, INV-001, AC-001..AC-003.

1. Owner directive 2026-10-14 (attribution from execution context), spec
   r2: if an invocation carries a `card_uid` and/or a `run_id`, record it;
   the requesting principal is always recorded. Unpair the gateway subject
   so `wyrd-run-id` and `wyrd-card-uid` are independent; plumb the
   execution's subject `card_uid` and optional `run_id` into the judge's
   gateway call (`ServerWyrdGatewayCaller`, which today builds
   `GatewayCallRequest { subject: None, .. }`); and record the requesting
   principal on Verifier executions. Add no binding enforcement or new
   authorization.
2. The gateway honors inbound W3C `traceparent`/`tracestate` on inference
   routes so its spans are children of the caller's span, and in-process
   calls made for a traced request inherit the active context. Malformed
   headers are ignored.

Evidence: journeys in Rust, Python, and TypeScript: a Run with an OTEL span
calls the gateway and a judge Verifier, then Bifrost shows the judge call with
the principal, the subject card_uid, and the run_id when present, joined to the
result carrying the same principal, and the gateway span sharing the caller's
trace_id; gateway calls for principal only, + card_uid, + run_id, and + both;
a judge execution without a Run; and a malformed traceparent.

Revision note (2026-10-14): item 1 amended per owner directive 2026-10-14
(attribution from execution context); the former "without a Run, leave
attribution absent" rule is dropped and the scope is limited to the three
items above.

## Evidence

Reuse: the gateway subject (`GatewayCallSubject`), its card-scope check
(`GatewayInvocation::attribute`), the capture rows, `ResultPayloadBuilder`, the
`wyrd.verifier_runs` requester column, and `TraceContext::parse_traceparent`
were extended in place; the only new owner is the task-local inbound trace
context middleware (`http/middleware/trace_context.rs`), which the gateway
read path had no equivalent for.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Gateway call records principal plus whichever of run_id/card_uid it carries | `gateway/ingress.rs::requested_subject`, `invocation.rs::attribute`, `capture.rs` | Rust journey `gateway_inference::gateway_call_records_whichever_attribution_it_carries` (4 cases); unit `components::gateway::ingress::tests::subject_headers_decode_independently`; `pg_invocation_tests` | PASS |
| Judge call carries subject card_uid and run_id when present (direct, observation, scheduled) | `verification/direct.rs::attributed`, `verification/eval.rs::judge_invoker`, `gateway/workflow.rs::ServerWyrdGatewayCaller::attributed` | Rust `verify_in_real_time::judge_call_is_attributed_to_the_run_and_card`, `…judge_call_without_a_run_joins_the_callers_trace_attributed_to_the_card`; Python `test_judge_call_is_attributed_to_the_run_and_card`; TS `judge call is attributed to the run and card` | PASS |
| Result records requesting principal and joins its judge calls on subject_card_uid | `tables/verification/results.rs`, `verification/results.rs::ResultRun.requested_by`, `components/verification/service.rs::stage_result` | the two Rust verify journeys above (result ↔ call join); unit `verification::results::tests::direct_task_writes_one_summary_row`; `tables::tests::verification_tables_match_their_approved_schemas` | PASS |
| Scheduled and observation runs record the tenant SYSTEM principal as actor (superseded wording; see the actor/origin follow-up) | migration `20261014000001_verifier_run_requester_every_origin.sql`, `queries/verifier_runs.rs` | `wyrd-sql` `pg_verifier_runs::*` (25) incl. `concurrent_schedulers_create_one_run_per_occurrence`; `pg_migration::*`; `wyrd-server` `pg_verification_runtime::*` | PASS |
| Gateway honors W3C traceparent/tracestate; in-process calls inherit; malformed ignored | `http/middleware/trace_context.rs`, `capture.rs::spans_batch` | Rust `gateway_span_joins_the_callers_trace`, `malformed_traceparent_is_ignored_by_the_gateway`, `malformed_traceparent_is_ignored_by_the_judge_call`; Python and TS gateway trace journeys; unit `http::middleware::trace_context::tests::*` | PASS |
| No new authorization or binding enforcement | `attribute` keeps the existing card-scope check, applied only when a card_uid is present; server-resolved judge subjects skip it | diff audit | PASS |

Commands: `scripts/postgres/with-test-postgres.sh` + `cargo nextest run -p wyrd-ai --test integration -P journey --run-ignored=all` (6/6),
pytest `-m integration` on the two Python journey files (3/3), vitest on the
two TS journey files (24/24), `mise run codegen:regen` then
`mise run codegen:check`, `mise run check:tenant-isolation`, `mise run fmt`,
`mise run fmt:check`, `mise run lints`, `mise run py:format`,
`mise run py:lints`, `mise run ts:format:check`, `mise run ts:lints`,
`mise run ts:typecheck`, `mise run docs:check`, `git diff --check`: all pass.

Limits: `wyrd.verifier_runs` rows enqueued before the migration keep a null
requester; the built-in `vala.verification.results` schema gains
`requested_by_principal_id` in place, following the existing built-in schema
precedent, with no evolution path for already-created tables.

### Follow-up (2026-10-14): client subject headers and caller trace propagation

Revision note: per coordinator follow-up, `wyrd-client` sends `wyrd-run-id`
and `wyrd-card-uid` independently, and SDK requests carry the caller's active
OpenTelemetry context (REQ-005).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Client sends run_id and card_uid independently | `wyrd-client` `workflow/gateway.rs::PublicWyrdGatewayCaller::with_subject` (each `Option`) | `wyrd-client` integration `workflow_transport::public_gateway_call_carries_run_and_card_uid` (both, none, run only, card only); Rust `gateway_call_records_whichever_attribution_it_carries`; Python `test_gateway_call_records_a_lone_run_or_card`; TS `gateway call records a lone run or card` | PASS |
| SDK requests carry the caller's trace context; traced verify joins the caller's trace | `wyrd-client` `transport/trace.rs` (one outbound pick: foreign-runtime scope, else current `tracing` span), `transport/http.rs::authenticated` (every HTTP request); Python and TS `verify` hand their active OTel span over with `with_caller_trace` | unit `transport::trace::tests::*`; Rust `traced_sdk_verify_joins_the_callers_trace`; Python `test_traced_verify_joins_the_callers_trace`; TS `traced verify joins the caller's trace` | PASS |
| MCP-initiated verifies inherit the inbound trace | `/mcp` serves each request on a spawned task, losing the edge's task-local context; `mcp/mod.rs::call_tool` re-scopes dispatch from the original request headers via `trace_context::within` | `wyrd-mcp` `connectivity::pg_tests::traced_tool_call_runs_under_the_callers_trace` (probe reports the inbound trace id) and `…streamable_http_client_reaches_authenticated_server_context` (no header, no trace) | PASS |

Limits: the header-map replay path (`authenticated_replay`) and gRPC send no
trace headers; Python and TypeScript bridge their OTel context for `verify`
only, since their other calls reach no traced server path.

Commands: `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=transport::trace::tests::no_active_context_sends_none) | test(=transport::trace::tests::scoped_caller_context_is_outbound)'` (2/2);
`mise exec -- cargo nextest run --locked -p wyrd-client --test integration -E 'test(=workflow_transport::public_gateway_call_carries_run_and_card_uid)'` (1/1);
`scripts/postgres/with-test-postgres.sh` + `cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(/judge_call|traceparent|joins_the_callers|whichever_attribution|traced_sdk_verify/)'` (7/7);
`scripts/postgres/with-test-postgres.sh` + `cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=connectivity::pg_tests::traced_tool_call_runs_under_the_callers_trace) | test(=connectivity::pg_tests::streamable_http_client_reaches_authenticated_server_context)'` (2/2);
pytest `-m integration -k 'joins_the_callers_trace or malformed_traceparent or judge_call_is_attributed or lone_run_or_card'` on the two Python files (5/5);
vitest on the two TS files (26/26); `mise run fmt`, `fmt:check`, `lints`,
`py:format`, `py:lints`, `py:typecheck`, `codegen:check`, `ts:format:check`,
`ts:lints`, `ts:typecheck`, `ts:napi:check` (after staging the regenerated
`index.d.ts`), `check:deps`, `docs:check`, `git diff --check`: all pass.

### Follow-up (2026-10-14): actor = SYSTEM for internal work; origin = references

Revision note: per coordinator follow-up, the actor recorded as
`requested_by_principal_id` is the authenticated caller for a manual run or a
direct execution, and the tenant SYSTEM principal for work Wyrd starts
internally (scheduled and observation-triggered runs). Origin is recorded only
as references (`run_id`, subject `card_uid`, `source_record_id`, binding, owner,
Trigger); the observation writer is no longer copied onto runs. The REQ-005
trace-on-replay/gRPC/Python-TS extension was cancelled and reverted; what
62be490c8 committed stays.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Scheduled and observation runs record SYSTEM; the writer is not copied | `queries/verifier_runs.rs` `INSERT_RUN_SQL` (manual: caller, else the tenant's active SYSTEM) and `INSERT_OBSERVATION_RUNS_SQL` (SYSTEM); `RunRequest::Observation` and `ObservationRecord` carry no principal; migration `20261014000001_verifier_run_requester_every_origin.sql` | `wyrd-sql` `pg_verifier_runs::observation_runs_are_unique_per_input_record`, `…observation_batches_insert_once_per_binding_and_record`, `…concurrent_schedulers_create_one_run_per_occurrence` | PASS |
| A direct execution records the caller | `components/verification/service.rs::stage_result`, `verification/results.rs::ResultRun.requested_by` | Rust `verify_in_real_time::judge_call_is_attributed_to_the_run_and_card`, `…judge_call_without_a_run_joins_the_callers_trace_attributed_to_the_card` (result requester = calling principal) | PASS |
| Observation-triggered execution span links (not parents) to the observation's stored trace/span | `verification/eval.rs::execute` calls `wyrd_telemetry::link_remote_span` on the `verification.engine` span when the record has both ids; `CapturedSpan.links` exposes links to tests | `wyrd-testing` `eval_verification::continuous_eval_reads_ordered_bounded_trace_evidence` (linked engine span stays in its own trace; spanless records link nothing) | PASS |

Decision: the check stays "manual requires requester"
(`verifier_runs_manual_requester_check`), not NOT NULL. Every tenant is
provisioned with SYSTEM (`platform/provisioning.rs`, backfill in
`20260601000028_system_principal.sql`), but nothing in the schema guarantees
it: `wyrd-dev-fixtures` `seed_tenant` creates tenants without one and the
runtime treats it as optional (`ClaimedRun.system_principal: Option<_>`,
`runner.rs` terminates such runs errored). An internal run of a tenant
without SYSTEM therefore records a null actor.

Limits: observation judge calls still run as the writer (`writer_caller`);
switching their authority to SYSTEM is deferred to the coordinator.

Commands: `scripts/postgres/with-test-postgres.sh` + `cargo nextest run --locked -p wyrd-sql --test integration --run-ignored=all -E 'test(/^pg_verifier_runs::/) | test(/^pg_migration::/)'` (46/46);
`mise run test:bifrost:integration:server` (86/86);
`scripts/postgres/with-test-postgres.sh` + `cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(/^eval_verification::/)'` (9/9);
the wyrd-ai journey filter (7/7), the MCP connectivity and gateway filter (5/5), Python (5/5) and TS (26/26) verify and gateway journeys;
`mise run fmt`, `fmt:check`, `lints`, `codegen:regen` then `codegen:check`,
`check:deps`, `check:tenant-isolation`, `docs:check`, `ts:napi:check`,
`py:typecheck`, `git diff --check`: all pass.
