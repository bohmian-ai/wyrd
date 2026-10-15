# TASK-001-R1 — Complete connection operation and failure contracts

Status: ready for `$wyrd-implement`.

## Authority and immutable inputs

- Approved specification: `changes/active/sdk-client-instantiation/spec.md`, revision 11.
- Original task: `changes/active/sdk-client-instantiation/tasks/TASK-001-unify-sdk-handle-instantiation.md`, original content at base `2f1db2194f5630df850b10987fe0f360946a525c`, interpreted under revision 11.
- Reviewed candidate: `07419b4d060303959384b682ca88e8673c722e63`; prior candidate: `fde7d396a3917964dc344ede2ed09046e179aa66`.
- Review: [verdict.md](verdict.md), [findings-validation.md](findings-validation.md).
- Finding: **FIND-TASK-001-4**, REVISED, VIOLATION. All other prior findings are closed.

This is a bounded implementation remediation under the existing approved behavior. No specification revision or additional credential source is needed.

## Diagnosis and selected correction

AGENTS §16 and agent rules require substantive rustdoc on materially changed Rust functions, including tests/helpers, with actual returned errors and remaining panic conditions. REQ-012 and AC-006 require accurate task-owned API documentation. Passing lint or broken-link checks does not establish those contracts. The prior remediation corrected many sites but left the following connected batch incomplete. Line numbers identify the reviewed candidate; use the named item if they move.

### Native insertion contract

`sdks/wyrd-sdk-python/src/bifrost/mod.rs:727–750`, `Bifrost::insert`, still says “Non-blocking” unconditionally and lists only Card UID, missing-table and queue-full errors. Its changed body calls `self.handle(py)?` before enqueue. An unopened `LazyConnection` performs synchronous shared connection there, including required configuration/authentication/transport work, with the GIL released. The documented first-use timing and errors therefore contradict the implementation.

Qualify nonblocking enqueue as applying after connection; describe initial synchronous setup and its existing error path. Reuse the already-correct explanation in `python/wyrd/bifrost/_client.py:704–723` and the owning connection accessor's actual behavior. Keep insertion and bounded queue behavior unchanged. No Python implementation edit is needed.

### Existing fallible helper channels

These three helpers already return `Result` and propagate surrounding IO, but their migrated eager connection now uses `.expect("handle connects")` without a panic contract:

| Owning item | Changed connection / reachable consumer |
|---|---|
| `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs:171`, `CardInvestigation::start` | Expect at :178; caller :415, `agent_discovers_service_and_component_cards`. |
| Same file :987, `judging_server` | Expect at :1021; caller :1094, `verification_tool_continues_trace_context`. |
| `crates/wyrd/wyrd-testing/tests/bifrost/oracle/workflow.rs:127`, `prove_forwarded_query_settles` | Expect at :159; caller :109–111, `workflow_forwarded_query_settles_before_the_run_ends`. |

Each provisions an ApiKey, so exchange refusal or unreachability can trigger the new panic. In the Oracle helper it also bypasses the caller's terminal-cause diagnostic context. Return this connection error through each helper's existing Result/error alias, following the corrected `EvidenceInvestigation::start` and SDK `principal_roles::author` pattern. Accurately include connection/exchange/setup failure in the existing error documentation. This reuses the established failure channel at its producer rather than adding downstream guards. Preserve all later operations, assertions, cleanup and shutdown.

### Complete the specifically changed test and writer contracts

Only the following additional validated items need documentation corrections; read each complete body and describe its actual scenario, returned failures and remaining panic conditions:

| Item at reviewed candidate | Required correction |
|---|---|
| `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs:433`, `an_agent_discovers_starts_and_observes_a_manual_run` | Add Panics for actual setup/assertion failures; existing Errors already describes returned failures. Its direct test setup expect can remain documented. |
| `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/gateway.rs:68`, `agent_administers_redacted_gateway_configuration` | Add Errors for returned bootstrap/configuration/HTTP/MCP/shutdown failures and Panics for retained assertions. |
| `crates/shared/wyrd-client/tests/integration/cards_transport.rs:187`, `list_uses_typed_query_parameters_and_an_empty_get_body` | Add Panics for setup/list/query-shape assertions. |
| Same file :239, `uid_delete_uses_kind_qualified_path_and_preserves_idempotence` | Add Panics for setup/delete/method/path/body assertions. |
| Same file :267, `server_problem_code_and_status_survive_registry_boundary` | Add Panics for setup/expected-refusal/code/status/details/request assertions. |
| `crates/shared/wyrd-client/tests/integration/pg_auth_e2e_against_fixture.rs:47`, `wyrd_client_authenticates_via_wyrd_access_token_header` | Add missing item scenario/workflow and Panics docs for setup/exchange/request failures and header/permission assertions. |
| `crates/wyrd/wyrd-server/tests/integration/pg_card_registration_route.rs:370`, `client_registration_saga_returns_active_receipt` | Add Panics for setup/registration/artifact/SQL expectations and receipt assertions. |
| `crates/shared/wyrd-client/tests/integration/startup_image_journey.rs:147`, `startup_image_write` | Add Panics for client/registration/gRPC/write/drain/state-file expectations and assertions. |
| Same file :232, `startup_image_verify` | Add Panics for saved-state parsing, client/artifact/read/drain failures and persisted-content assertions. |
| `crates/wyrd/wyrd-testing/src/bifrost/write.rs:58`, `BifrostWriter::connect` | Extend existing Errors to cover required credential-exchange refusal/unreachability before native Bifrost assembly. |

Gateway, Cards transport and card-registration fixtures use bearer credentials: their HTTP-only connection does not exchange or dial. The auth fixture explicitly primes its API-key cache before connection. Do not invent new connection failures in those descriptions. The violation at those sites is the mandatory changed-item documentation gap, not proof of a newly reachable authentication failure.

## Constraints and preserved behavior

- Keep the write set to the named Rust items: documentation plus connection-error propagation in the three fallible helpers. Reuse existing types and mechanisms; add no helper, error type, abstraction, checker, fixture, dependency or test harness.
- Keep synchronous first insertion, bounded admission and refusal, lazy construction, local property/view access, connection ownership, caches and shared resolver unchanged.
- Preserve all assertions, permissions, retained audit evidence, recovery/sibling-availability checks, restart proof and cleanup. Do not suppress a failing check or weaken a test.
- Preserve prior findings 1/2/3/5/6 closure. Approved Rust injected-environment proof and Platform credential/session behavior remain accepted.
- Exclude new auth sources, Platform persistence, auth probes, preconnection buffering, awaitable insert, lock/lifecycle redesign, unrelated rustdoc cleanup and pure name-only migrations. TASK-002 continues to own docsite/README work.

## Acceptance mapped to FIND-TASK-001-4

1. Native insertion docs accurately distinguish initial connection from connected enqueue and cover existing first-use errors.
2. Each of the three fallible helpers returns eager connection failures through its existing Result channel, and its docs match the final behavior without losing later assertions or cleanup.
3. Every additional item in the exact table has substantive, accurate operation/error/panic documentation as applicable. No invented bearer/cache exchange failure is claimed.
4. Source comparison confirms every named final body/doc pair, previously corrected sites remain closed, and the focused checks below pass. No runtime capability or approved contract changes.

## Focused proof

Documentation-only corrections need source verification against full bodies, not new runtime tests. For the three helper propagation changes, run the existing exact journeys using the repository-managed Postgres setup and migration used by their owning lanes:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-mcp --test mcp -P journey --run-ignored=all \
  -E 'test(=verification::pg_tests::agent_discovers_service_and_component_cards)'"

scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-mcp --test mcp -P journey --run-ignored=all \
  -E 'test(=verification::pg_tests::verification_tool_continues_trace_context)'"

scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-testing --test oracle -P journey --run-ignored=all \
  -E 'test(=workflow::workflow_forwarded_query_settles_before_the_run_ends)'"

mise run fmt
mise run lints
git diff --check
```

Record the exact commands and results. Run no full journey aggregate or broad gate for this remediation. Staying within this Rust-only write set does not require Python/TypeScript/codegen/declaration regeneration. A later independent task review reassesses the cumulative candidate against the original task and approved specification.

## Implementation evidence

| Acceptance | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| 1. Native insert docs | `sdks/wyrd-sdk-python/src/bifrost/mod.rs` `Bifrost::insert`: first insert connects through `Bifrost::handle` (GIL released); non-blocking enqueue once connected; Errors adds the connection error | Source comparison with body (`self.handle(py)?`) and `_client.py` insert docs | PASS |
| 2. Helper propagation | `CardInvestigation::start`, `judging_server` (`verification.rs`), `prove_forwarded_query_settles` (`workflow.rs`): `.expect("handle connects")` → `.await?` through existing `McpJourneyError`/`JourneyError`; Errors docs name exchange/connection failure | Three focused journeys below | PASS |
| 3. Tabled docs | `an_agent_discovers_starts_and_observes_a_manual_run` Panics; `agent_administers_redacted_gateway_configuration` Errors/Panics; three `cards_transport.rs` tests Panics; `wyrd_client_authenticates_via_wyrd_access_token_header` scenario/Panics; `client_registration_saga_returns_active_receipt` Panics; `startup_image_write`/`startup_image_verify` Panics; `BifrostWriter::connect` Errors. Bearer and primed-cache sites state that connecting performs no exchange | Source comparison against each full body | PASS |
| 4. No contract change | Docs plus three `?` propagations only; no helper, type, dependency, or behavior change | `mise run fmt`, `mise run lints`, `git diff --check` clean | PASS |

Commands (each passed, 1 test):

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=verification::pg_tests::agent_discovers_service_and_component_cards)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=verification::pg_tests::verification_tool_continues_trace_context)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=workflow::workflow_forwarded_query_settles_before_the_run_ends)'"
mise run fmt && mise run lints && git diff --check
```

Write set limited to the named items; prior findings 1/2/3/5/6 untouched. No
Python, TypeScript, codegen, or declaration change.
