---
id: TASK-006-R1
kind: remediation
status: ready
spec: SPEC-example-journey-fixes
spec_revision: 4
requirements: [REQ-001, REQ-002, REQ-004, REQ-005, REQ-006, AC-001, AC-003, AC-004, OD-1, OD-3, OD-4, OD-7]
parent_tasks: [TASK-001, TASK-002, TASK-003, TASK-004, TASK-005]
remediates:
  - FIND-example-journey-fixes-1
  - FIND-example-journey-fixes-2
  - FIND-example-journey-fixes-3
  - FIND-example-journey-fixes-4
  - FIND-example-journey-fixes-5
  - FIND-example-journey-fixes-6
  - FIND-example-journey-fixes-7
  - FIND-example-journey-fixes-8
  - FIND-example-journey-fixes-9
---

# Remediate integrated example-journey acceptance gaps

## Subject and authority

- Approved spec: `changes/active/example-journey-fixes/spec.md`, revision 4.
- Binding decisions: `changes/active/example-journey-fixes/owner-decisions.md`,
  OD-1 through OD-7.
- Original tasks:
  `changes/active/example-journey-fixes/tasks/TASK-001-jev-typed-provider.md`,
  `TASK-002-bifrost-errors-and-subqueries.md`,
  `TASK-003-mcp-compatibility.md`,
  `TASK-004-gateway-attribution-and-trace.md`, and
  `TASK-005-local-keyring.md` as replaced by OD-5.
- Review base: `ef03f624a1f3a6bcd51bbd17060271f0b5e8cabf`.
- Reviewed candidate: `c4ba2b0502f8968a78e5e960bcc9eba43e960643`.
- Validated ledger:
  `changes/active/example-journey-fixes/review/codex-integrated-review/findings-validation.md`.

Route this task directly to `$wyrd-implement`. It is one remediation task
because final acceptance must be re-established against one cumulative
candidate, but each correction stays with the existing owner named below.

## Diagnoses and required outcomes

### 1. Direct execution loses a successful result without SYSTEM

`FIND-example-journey-fixes-1` is `INCORRECT`. HTTP, MCP and SDK direct
execution reach `VerificationControl::execute`. The engine and judge can finish,
then `stage_result` returns early when `DirectTarget.system_principal` is absent
(`crates/wyrd/wyrd-server/src/components/verification/service.rs:417-510,523-638`).
The caller receives success although no result, requester or joinable judge
evidence is retained. The accepted nullable-SYSTEM exception covers internally
started work, not authenticated direct execution.

Require the existing result-writer SYSTEM identity during `resolve_direct`,
before engine or provider work. Reuse the current SYSTEM lookup and existing
`WyrdError::ServiceUnavailable` with reason `system_principal_missing`. Keep the
authenticated caller as `ResultRun.requested_by`; keep SYSTEM only as the
internal `VerifierAttribution.principal`. Do not publish through the caller and
do not change scheduled/observation behavior.

### 2. The motivating external example has no acceptance proof

`FIND-example-journey-fixes-2` is `MISSING`. AC-004 requires
`bohmian-ai/wyrd-example-ts` to complete without client-side escaping or an MCP
tool allow-list. Component journeys do not prove the external application's
dependency versions, setup, agent loop, telemetry, gateway, verification,
Bifrost and MCP composition.

Run the unmodified official example at an immutable revision against the
cumulative remediation candidate using its documented workflow. Record the
example commit, exact commands and configuration, credential-gated limitations,
terminal result, and evidence that neither workaround is present. If it exposes
a product defect, correct only that existing owning path and repeat its focused
proof before repeating the example.

### 3. Client-side trace injection contradicts OD-3

`FIND-example-journey-fixes-3` is a `VIOLATION`. The new outbound task-local and
shared authenticated request builder synthesize trace headers for every
authenticated client request, with Python and Node bridges feeding the same
path (`wyrd-client/src/transport/trace.rs`, `transport/http.rs:876-904`, and the
foreign-runtime sites named in the validated ledger). OD-3 explicitly says no
further client-side injection.

Delete that outbound trace module/task-local, the shared HTTP injection, the
Python active-trace bridge used for verify, the Node trace handoff, and tests
whose sole contract is automatic SDK propagation. Preserve the server inbound
middleware, MCP re-scoping of headers already received, in-process judge
inheritance, observation span links, and explicit-header HTTP behavior. Add no
replacement option, flag or transport layer.

### 4. Unified Jev adds validation OD-4 excludes

`FIND-example-journey-fixes-4` is a `VIOLATION`. Unified `chat_request`
deserializes the typed last-user payload and then calls `checked`, applying the
native request's non-empty/count/name bounds (`adapter/jev.rs:63-95` and
`wire/jev.rs:168-247`). OD-4 requires typed deserialization with no per-field
validation on this pass-through.

Return the typed `JevRequest` directly after unified-route deserialization.
Keep closed enums and unknown-field rejection. Keep `checked` and all existing
bounds on native `/v1/systemone`. Do not add another request type, mapper or
chat-to-question translation.

### 5. New Jev methods fail the hard rustdoc gate

`FIND-example-journey-fixes-5` is a `VIOLATION`. `JevClient::send` and the Vala
test provider's `send`/`stream` are fallible but lack required `# Errors`
sections; the test `send` also hides two panic invariants
(`clients/jev.rs:134-139`; `orchestrator_judge_skald.rs:440-458`).

Document the forwarded native-client failures, the test sender's absence of an
ordinary error outcome, the deliberate stream timeout, and the poisoned-lock
and invalid-static-answer panic conditions. Change no behavior and add no
helper.

### 6. Historical internal verifier runs are not backfilled

`FIND-example-journey-fixes-6` is `MISSING`. The old schema forced scheduled
and observation requesters to null. Migration
`20261014000001_verifier_run_requester_every_origin.sql` only replaces the
constraint even though the preceding SYSTEM migration already supplies the
same-tenant actor for existing ordinary tenants.

In the existing migration, before installing the replacement constraint,
backfill null `schedule` and `observation` requesters from the same tenant's
active SYSTEM principal. Preserve manual requesters, tenant isolation, and null
only when no SYSTEM principal is resolvable. Do not touch the separately
owner-accepted built-in results-table evolution gap.

### 7. Invalid W3C tracestate is retained

`FIND-example-journey-fixes-7` is `INCORRECT`. Inbound middleware joins
visible-ASCII `tracestate`, while `TraceContext::parse_traceparent` validates
only the parent and copies the state (`trace_context.rs:52-70`;
`wyrd-spec/src/trace.rs:19-44`). Invalid state can therefore enter retained
span evidence.

At the existing inbound middleware boundary, parse the combined state with the
already-installed OpenTelemetry `TraceState` parser. If state is invalid, keep
the valid parent with empty state. Do not refuse the request, invalidate the
parent, add a second parser/carrier, or turn the W3C 512-character propagation
recommendation into an unapproved hard limit.

### 8. Two required Bifrost caller faults lack real journeys

`FIND-example-journey-fixes-8` is `MISSING`. Current SDK journeys exercise
misspelled columns and MCP adds unknown table/function, but no real query makes
DataFusion plan a type mismatch or ambiguous reference. Those shapes can take
different wrappers through `caller_planning_fault` and regress to opaque 5xx.

Extend the existing query fixtures and the Rust, Python, TypeScript and MCP/HTTP
journeys with one planning-time type mismatch and one ambiguous reference.
Reuse the shared Oracle mapper. If either case exposes a missing wrapper,
correct `caller_planning_fault` once. Add no query-shape guard or new harness;
keep internal and unauthorized failures opaque.

### 9. Live subqueries never prove managed run_id correlation

`FIND-example-journey-fixes-9` is `MISSING`. All current live subquery journeys
use ordinary `id`, and the analytical workload deliberately substitutes
`run_key`. They do not exercise Wyrd's stamped, projected managed `run_id`.

Reuse the existing public run/observation fixture path to create live rows that
share and differ in actual managed `run_id`. Add one correlated scalar, `IN` or
`EXISTS` subquery and its equivalent join through the existing Rust, Python,
TypeScript and MCP fixtures. Do not add a test-only SQL path or second query
engine.

## Constraints and preserved behavior

- Preserve every PASS entry in `verdict.md`, especially MCP compatibility,
  tenant isolation, non-blocking audit and deployment-keyring behavior.
- Preserve the three owner-held decisions: no existing-table evolution for the
  built-in results schema; null actor for an internal run when no active SYSTEM
  exists; observation-triggered judge authority remains open and unchanged.
- Preserve native Jev validation, typed request/response shapes, both approved
  routes, shared refusal behavior, non-retry semantics, credential scrubbing,
  fallback isolation and stable errors.
- Preserve explicit inbound W3C context, MCP task re-scoping, in-process trace
  inheritance, and observation span links. Trace never authorizes or attributes.
- Preserve Bifrost authorization before planning and opaque internal errors.
- Make no public API, new provider, compatibility alias, new Verifier semantic,
  schema family, trace transport option, secret mechanism or query engine.
- Do not weaken, ignore or delete a gate or unrelated test. Do not broaden the
  remediation because an adjacent refactor is attractive.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-1` | Direct execution without SYSTEM refuses with stable 503 before engine/provider work; normal direct execution still retains caller, result and judge join. |
| `FIND-2` | A pinned, unmodified external example completes and the evidence demonstrates no escaping or tool allow-list workaround. |
| `FIND-3` | Ambient SDK spans synthesize no trace headers; explicit inbound and in-process server trace behavior remains green. |
| `FIND-4` | Unified typed Jev requests that violate a native-only custom bound reach the provider; malformed JSON/unknown discriminator still refuse once; native bounds remain. |
| `FIND-5` | Every named method has substantive required `# Errors`/`# Panics` documentation. |
| `FIND-6` | Migration backfills same-tenant SYSTEM for old scheduled/observation rows, preserves manual requester and tenant isolation, and leaves null only without SYSTEM. |
| `FIND-7` | Invalid W3C state is dropped while a valid parent survives; valid repeated-header state remains ordered. |
| `FIND-8` | Each shipped query surface returns `WYRD_VALA_400_QUERY_INVALID_SQL` with actionable planner detail for real type mismatch and ambiguity. |
| `FIND-9` | Each shipped query surface returns identical expected live rows for a managed-`run_id` subquery and equivalent join. |

## Focused proof

Use the repository-pinned toolchain and Postgres wrapper where required. Exact
new test names below are part of this task so a positional filter cannot pass
after selecting nothing.

```bash
# Direct execution readiness and unchanged direct success.
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked \
  -p wyrd-ai --test integration -P journey --run-ignored=all \
  -E 'test(=verify_in_real_time::direct_execution_without_system_refuses_before_work) | test(=verify_in_real_time::judge_call_without_a_run_joins_the_callers_trace_attributed_to_the_card)'

# Shared client emits no ambient context after the OD-3 deletion.
mise exec -- cargo nextest run --locked -p wyrd-client --test integration \
  -E 'test(=workflow_transport::public_gateway_call_does_not_inject_ambient_trace_context)'

# Unified/native Jev boundary and provider/judge regression tests.
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib \
  -E 'test(=adapter::tests::jev_chat_pass_through_carries_the_typed_request_and_answer) | test(=adapter::tests::jev_bad_forms_are_rejected_before_provider_io)'
mise exec -- cargo nextest run --locked -p wyrd-skald-providers -p vala-eval

# Historical requester migration.
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked \
  -p wyrd-sql --test integration --run-ignored=all \
  -E 'test(=pg_migration::verifier_run_requester_migration_backfills_resolvable_system_actors)'

# W3C parent/state separation.
mise exec -- cargo nextest run --locked -p wyrd-server --lib \
  -E 'test(/^http::middleware::trace_context::tests::/)'

# Rust and MCP Bifrost caller faults plus managed-run correlation.
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked \
  -p wyrd-ai --test integration -P journey --run-ignored=all \
  -E 'test(=query_bifrost::unknown_column_carries_the_planners_suggestion) | test(=query_bifrost::live_table_subqueries_match_their_joins)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked \
  -p wyrd-mcp --test mcp -P journey --run-ignored=all \
  -E 'test(=query::pg_tests::bifrost_query_errors_are_actionable_before_row_execution) | test(=query::pg_tests::live_table_subqueries_answer_like_joins)'

# The same focused existing files own Python and TypeScript projections.
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:all:inner && mise run py:setup && cd sdks/wyrd-sdk-python && uv run pytest tests/integration/test_query_bifrost.py -k "unknown_column or live_table_subqueries"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:all:inner && mise run ts:build && mise run ts:build:testing && cd sdks/wyrd-sdk-ts/wyrd && pnpm vitest run tests/integration/query-bifrost.test.ts -t "unknown column|live table"'
```

Also run only the write-set static lanes: `mise run fmt`, `mise run lints`,
`mise run codegen:check` if generated declarations change, `mise run
py:format:check`, `mise run py:lints`, `mise run py:typecheck` for the Python
bridge deletion, and `mise run ts:format:check`, `mise run ts:lints`, `mise run
ts:typecheck`, `mise run ts:napi:check` for the Node boundary deletion. Run
`mise run check:tenant-isolation` for the migration. Do not run a broad gate or
the full capability journey suites in this remediation task; final change
review owns the aggregate rerun.

Record the external example's own documented exact commands separately with
its immutable commit because it is outside this repository's `mise.toml`.

## Non-goals

- No replacement client trace propagation mechanism.
- No change to observation-triggered judge authority.
- No compatibility path for the rejected gateway keyring or old Jev shim.
- No expansion of credential-body encoding heuristics.
- No new Bifrost workload generator, SQL parser or error taxonomy.
- No cleanup of unrelated imports, documentation or test infrastructure.

## Implementation evidence (task/TASK-006-R1)

Owner scope decisions applied: FIND-1 and FIND-6 closed as superseded by
`sub-act-attribution` (OD-1 note); FIND-2 excluded (external example).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-3: `traceparent`/`tracestate` only on verify; no shared-builder injection or bridge | `wyrd-client` `transport/trace.rs` deleted, `transport/http.rs` restored; `observe/verify.rs:171,206`; `observe/mod.rs:414,442,465`; Python `observe/mod.rs:430`; TS `native/src/cards.rs:705` | `wyrd-client --test integration` `workflow_transport::public_gateway_call_does_not_inject_ambient_trace_context`, `public_gateway_call_carries_run_and_card_uid`; Python `test_traced_verify_joins_the_callers_trace`; TS `traced verify` | PASS |
| FIND-4: unified Jev route is typed pass-through only | `wyrd-gateway/src/adapter/jev.rs:66`; `adapter/tests.rs:3350,3705` | `wyrd-gateway --lib` `adapter::tests::jev_bad_forms_are_rejected_before_provider_io`, `jev_chat_pass_through_carries_the_typed_request_and_answer` | PASS |
| FIND-5: rustdoc `# Errors`/`# Panics` | `skald-providers/src/clients/jev.rs`; `vala-eval/tests/integration/orchestrator_judge_skald.rs:444,449,468` | `cargo nextest run -p wyrd-skald-providers -p vala-eval`; `mise run lints` | PASS |
| FIND-7: malformed `tracestate` dropped, valid parent kept | `wyrd-server/src/http/middleware/trace_context.rs:74,124` | `wyrd-server --lib` `http::middleware::trace_context::tests::*` (3) | PASS |
| FIND-8: type mismatch and ambiguous reference map to the planner's detail | Rust `query_bifrost.rs:365`; Python `test_query_bifrost.py:163`; TS `query-bifrost.test.ts:177`; MCP `query.rs:339,344` | exact Rust, MCP, Python and TS journey selectors | PASS |
| FIND-9: live subquery correlation on managed `run_id` | Rust `query_bifrost.rs:293`; Python `test_query_bifrost.py:114,121`; TS `query-bifrost.test.ts:140`; MCP `query.rs:471`; `wyrd-testing` `bifrost/write.rs`, `query_fixture.rs` | same journey selectors | PASS |
| AC-001 gap: Jev Prompt Card `llm_judge` journey | `fixtures/cards/gateway_jev/jev-judge-prompt.yaml`, `reply-quality.yaml`; Rust `gateway_jev.rs:276,610,658`; Python `test_gateway_jev.py:377,425`; TS `gateway-jev.test.ts:223,258` | `gateway_jev::*` (8), `test_gateway_jev.py` (8), `gateway-jev.test.ts` (8) | PASS |
| Gate: `py:typecheck` | `test_query_bifrost.py` typed permission dict | `mise run py:typecheck` | PASS |
| Gate: `docs:check` | `docs/public/llms-full.txt` regenerated | `mise run docs:check` | PASS |
| Gate: `server_dev_repeat_and_partial_failure_preserve_state` | `wyrd-cli/tests/pg_tests.rs:302` picks ports below the ephemeral range | `wyrd-cli --test cli -E 'test(/^pg_tests::/)'` x3 | PASS |
| Gate: `test:identity:journey` dex exit | none (host: Colima VM does not mount `/tmp` worktrees; bind source appears as an empty directory) | `docker compose logs dex` | REPORTED |

### Diagnosis: server_dev retry "WYRD_SERVER_BIND is occupied"

- **Symptom:** the retry in `server_dev_repeat_and_partial_failure_preserve_state` fails at `pg_tests.rs:595` because the `server dev` listener reservation (`dev.rs:242`) cannot bind. This happened only under the full parallel gate.
- **Evidence:** gate.log lines 3045-3060. Isolated runs pass. `DevArgs::run` always calls `stop()`, which reaps the child before the CLI exits (`dev.rs:103,400-444`).
- **Cause:** `free_bind()` binds `127.0.0.1:0` and drops the socket. That returns the port to the kernel's ephemeral range, where any concurrent `bind(:0)` or outbound connect can take it while the port sits between runs.
- **Fix site:** the `free_bind()` fixture. It now picks probed ports below `ip_local_port_range`, which the kernel never assigns automatically. An independent read-only diagnostician confirmed the cause. The same pattern exists in `identity_ui_e2e.rs:92`, `pg_grpc_ingest_smoke.rs:257`, `pg_grpc_smoke.rs:164` and Python `test_local_development.py:223`, but those were not red and are unchanged.
