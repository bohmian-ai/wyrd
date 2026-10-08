---
id: TASK-008
kind: implementation
status: superseded
superseded_by: task-008-recovery.md
spec: SPEC-verified-change-contract
spec_revision: 48
requirements: [REQ-089, REQ-101, REQ-114, REQ-151, REQ-152, INV-015, AC-017, AC-020, AC-021, AC-022, AC-023, AC-024, AC-030, AC-032, AC-033]
depends_on: [TASK-005, TASK-006, TASK-007, TASK-009, TASK-010]
---

## Outcome and Value

The complete Verifier change is proven as one production-shaped product across
Rust, Python, TypeScript, HTTP, MCP, multi-server Bifrost, Postgres restart,
authorization, tenant isolation, resource ceilings, and current architecture.
This task closes cross-task seams and missing journey registration; it adds no
new durable behavior or alternate implementation.

## Owners, Scope, Consumers, and Prohibited Changes

`wyrd-testing` owns production-shaped servers, clusters, telemetry capture, and
capability journey registration. Each language runtime owns its lifetime-
dependent tests. Existing `mise` capability lanes own environment setup.
Architecture and generated contracts are static closure.

Do not repair a failing journey by weakening assertions, mocking away a real
server/dependency, adding sleeps, changing approved behavior, or building a
second harness. Do not add product code solely to satisfy a test unless the
test reveals an in-scope missing seam owned by TASK-001 through TASK-007 or
TASK-009.

## Approach

1. Audit every REQ/INV/AC against landed focused proof and identify only
   missing cross-owner seams.
2. Add/complete the minimum real SDK/server journeys in existing harnesses and
   register them in current `mise` lanes.
3. Exercise multi-server, restart, concurrency, authorization/audit, partial
   result, provider, and analytical pruning scenarios without credentials in
   fast lanes.
4. Regenerate all public artifacts and remove stale architecture/build
   references to retired surfaces.
5. Run the complete affected capability gates sequentially and record exact
   evidence for final change review.

## Proof Strategy

TDD is not applicable because this task is verification-only: executable
behavior is introduced by TASK-001 through TASK-007 and TASK-009. Each missing
journey is a regression/conformance proof against already-required behavior;
it must fail first for the expected missing seam or absent test registration,
then pass without inventing a new product contract.

The integrated proof matrix must include:

- Rust/Python/TypeScript registered Service journeys containing PSI, SPC,
  Custom, deterministic Eval, and local LLM-judge Eval bindings; each crosses
  exact-principal authentication, locked run API, queue/IPC, Gate/Scribe,
  runtime, Bifrost result query, status, and one failed-result Operator.
- Python framework-created spans inside `with state.run(card="...")` carrying
  the exact run ID and CardRef through OTLP to server-resolved Bifrost Card UID,
  including persisted joins to caller-owned custom rows by `run_id` and Eval
  rows by trace/span identity, nested Card scopes, asyncio isolation,
  private-provider install, and fail-open absence or failure of optional
  OpenTelemetry integration.
- HTTP/MCP manual binding and direct Drift runs with requester identity,
  nullable direct owner/binding, idempotency, unauthorized and cross-tenant
  refusals, and existing Bifrost query for result/detail rows.
- Multi-server result writing where the worker owns no Scribe, including the
  complete internal tenant SYSTEM identity/table matrix, public principal and
  credential-path refusals, single existing-issuer/JWT reuse, one Gate audit
  decision, and one raw observation feeding two independently filterable
  bindings.
- Daily partitions and physical Bloom evidence across two time partitions,
  Oracle partition/row-group pruning, exact Eval record-day lookup, and one
  result event time across ACKs straddling UTC midnight.
- Duplicate record/batch/cron handling, expired leases, retry exhaustion,
  fail-open Eval enqueue, partial result visibility without settlement,
  zero-detail summary-only publication, restart recovery, and no stale old
  table/route/crate path.
- PostgreSQL-owned activity, schedule, claim, lease, retry, and dispatch
  deadlines, with no process wall-clock predicate, skew workaround, injectable
  coordination clock, or permanent source checker.
- Permission allow/deny audit at every public/Gate boundary, no audit for
  internal mechanics, per-tenant/global saturation fairness, Operator timing
  ceilings, shutdown drain/reclaim, supervisor health, and required telemetry.
- Operator connection CRUD/rotation/redaction and local Slack/PagerDuty/HTTP
  protocol fixtures, including typed path IDs, secret-free CLI argv/debug,
  served OpenAPI, and runtime MCP catalogs. Credentialed live smokes remain
  release-gated evidence.
- Exact Card-bound API-key and workload-`jwt-bearer` exchanges activate owners;
  delegation, human refresh, Card-free automation, SYSTEM issuance, cached
  bearer requests, and idle expiry do not. Principal suspension blocks new
  work immediately while ordinary authorization retains the current
  five-minute permission-snapshot semantics.

## Acceptance Criteria

- Every spec obligation maps to a passing strongest applicable proof or an
  explicitly inherited static contract; no acceptance criterion relies only on
  a lower tier when a journey is required.
- All user-facing surfaces use the same server-owned durable behavior and
  stable errors.
- The current tree contains no stale authoritative alternative, generated
  drift, retired route/table/crate/check reference, or unregistered journey.
- No material decision is made in this task; discoveries requiring one return
  to `$wyrd-spec`.

## Expected Write Set and Consumer Closure

Expected changes are limited to existing journey/integration test homes,
capability registration in `mise.toml` when missing, generated artifacts from
their sources, and permanent architecture/public documentation required by
`REQ-114`. Production changes are allowed only as bounded corrections to a
missing in-scope seam in its established owner and must be recorded.

## Verification and Evidence

Run Cargo-backed work sequentially. Record every specifically named test with
its exact focused command. The integrated lanes are:

```bash
mise run test:cards:integration
mise run test:principals:integration
mise run test:sql
mise run test:shared
mise run test:vala
mise run test:wyrd
mise run test:wyrd-sdk
mise run verify:bifrost
mise run test:wyrdstate:journey
mise run test:platform:journey
mise run test:cli:journey
mise run test:bifrost:journey:sdk
mise run test:bifrost:journey:server
mise run test:bifrost:journey:mcp
mise run test:bifrost:journey:python
mise run test:bifrost:journey:typescript
mise run py:test:unit
mise run py:test:integration
mise run py:typecheck
mise run ts:test:unit
mise run ts:test:integration
mise run ts:typecheck
mise run ts:napi:check
mise run codegen:check
mise run check:client-tier
mise run check:pyo3-scope
mise run check:tenant-isolation
mise run check:registry-tx-coupling
mise run check:from-pools-allowlist
mise run check:unwrap-audit
mise run fmt
mise run py:format
mise run lints
mise run py:lints
git diff --check
```

This change is intentionally broad; after all capability lanes pass, run
`mise run gate` as whole-plan closeout evidence.

## Material Stop Conditions

Stop for any need to change a public/persisted contract, security or tenancy
semantics, reliability ceiling, accepted non-goal, dependency direction, or
test tier requirement. A proven unrelated baseline failure is recorded and
does not justify weakening proof.

## Authority Links

- `changes/active/verified-change-contract/spec.md`
- all documents under `changes/active/verified-change-contract/architecture/`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/bifrost-design.md`
- `architecture/wyrd-security-posture.md`
- `architecture/references/languages/testing-workflows.md`
- `AGENTS.md`

## Implementation Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Rust/Python/TypeScript Service journeys with PSI, SPC, Custom, deterministic Eval, and local LLM-judge Eval bindings, each ending in a delivered failed-result Operator read from Run GET | `sdks/wyrd-sdk-rust/tests/drift_verification.rs`; `sdks/wyrd-sdk-python/tests/integration/test_drift_journey.py`; `sdks/wyrd-sdk-ts/tests/integration/drift-verification.test.ts`; judge rooted at the gateway mock (`WyrdTestServer::with_gateway_provider_root_for_test`) | `service_verifies_drift_and_eval_through_the_sdk`; `test_service_bindings_verify_drift_and_eval_through_an_http_operator`; "verifies one Service through Drift, Eval, and an Operator end to end" | PASS |
| Python spans inside `state.run(card=...)` carry run ID and CardRef through OTLP; joins to custom rows by `run_id` and Eval rows by span; nested scopes, asyncio isolation, invalid hex, stale-writer fence | `test_observe_journey.py`; `observe_run.rs`; `observe-run.test.ts` | `scoped_run_emits_drift_eval_and_generic_rows`; Python and TS observe journeys | PASS |
| AC-021 payload-form parity and refusal of values the wire cannot represent | `wyrd-client/src/observe/drift.rs::check_integer_literals` (bounded correction) | `observe::tests::drift_json_refuses_integer_literals_beyond_64_bits`; `test_drift_refuses_unrepresentable_payloads_before_admission`; `test_drift_accepts_a_pydantic_model` | PASS |
| HTTP/MCP manual and direct runs: requester identity, null direct owner/binding, idempotency, unauthorized and cross-tenant refusals, Bifrost result query | `pg_verification_routes.rs`; MCP `an_agent_runs_a_verifier_directly_and_reads_its_result`; `verification_run.rs::assert_verifier_kind_contract` | `test:wyrd`, `test:bifrost:journey:mcp`, `test:bifrost:journey:sdk` | PASS |
| Multi-server result writing, SYSTEM identity/table matrix, public principal refusals, one raw observation feeding two bindings | `pg_grpc_ingest_smoke.rs::{forged_tenant_result_writes_are_refused, system_writer_matrix_spans_every_builtin_table, system_owner_token_cannot_write_a_customer_result}`; `verification_runtime.rs::two_bindings_share_one_client_observation`; `pg_verification_routes.rs::system_writer_token_is_refused_by_every_public_token_grant` | `verify:bifrost`, `test:wyrd` | PASS |
| SYSTEM-token contract violations return client errors, not 500s | `wyrd-auth-verify/src/lib.rs` (SYSTEM_OWNER tenant -> 401); `wyrd-auth/src/exchange_api_key.rs::delegation_chain` (SYSTEM subject -> 400 `WYRD_SPEC_400_VALIDATION`, `details.reason="subject_is_system"`) (bounded corrections) | `tests::into_verified_rejects_every_system_contract_violation`; `system_writer_token_is_refused_by_every_public_token_grant` | PASS |
| A pending registration operation with NULL `stored_response` decodes, so the card reconciler no longer fails with `WYRD_REGISTRY_503` | `wyrd-sql/src/queries/cards/register.rs` drops `#[sqlx(json)]` from `stored_response` (bounded correction) | `pg_cards_register::pending_operation_with_null_stored_response_decodes`; `WYRD_LOG=info mise run test:cards:integration` logs no `WYRD_REGISTRY_503`; `test:sql` | PASS |
| Daily partitions, result Bloom columns, partition and row-group pruning, one result event time across ACKs | `vala-bifrost-redux/src/tables/mod.rs` schema test pins the managed envelope and Bloom union; `verification_runtime.rs::result_layout_partitions_blooms_and_prunes_by_result` | `verify:bifrost` | PASS |
| AC-012, AC-014, AC-024 results join details by `result_id` within the caller's tenant-scoped query (spec revision 48 erratum; `table_schema.md` lists only the managed columns Bifrost writes) | `vala-bifrost-redux/src/tables/mod.rs` schema test pins the managed envelope; Drift and Eval journeys read details by `result_id` through tenant-bound queries | `verify:bifrost`, `test:bifrost:journey:sdk`, `test:bifrost:journey:python`, `test:bifrost:journey:typescript` | PASS |
| Restart, shutdown drain, lease reclaim, retry, saturation fairness, supervisor health, secret-free telemetry | `pg_operator_delivery.rs` and `pg_verification_*` tests listed in the task scope; `wyrd-testing/src/logs.rs::LogCapture` | `test:operators:integration`, `test:wyrd` | PASS |
| PostgreSQL-owned activity, schedule, claim, lease, and dispatch deadlines | activity stamp bracketed by `statement_timestamp()` in `pg_verification_bindings.rs`; tests move DB rows (`make_binding_due`, `make_retries_due`) | `test:wyrd` | PASS |
| Operator connection CRUD, rotation, redaction, Slack/PagerDuty/HTTP fixtures, CLI, MCP, cross-tenant | `pg_operator_connection_routes.rs`; `operator_connections.rs` (Rust SDK); `operator-connections.test.ts`; CLI `cli_manages_redacted_operator_connections`; MCP `an_agent_administers_redacted_connections` | `test:operators:integration`, `test:wyrdstate:journey`, `test:cli:journey`, `test:bifrost:journey:typescript` | PASS |
| Exact Card-bound exchanges activate owners; delegation, SYSTEM mint, and lapse do not | `pg_verification_bindings.rs::runtime_activity_follows_only_qualifying_exchanges` | `test:wyrd` | PASS |
| Verifier Card contract refusals leave no writes; Workflow Operator refused in `on_failure` | `pg_card_registration_route.rs::verifier_contract_refusals_leave_no_writes`; CLI `cli_enforces_the_verifier_card_contract` | `test:cards:integration`, `test:wyrd`, `test:cli:journey` | PASS |
| No stale retired route/table/check reference; journeys registered | `vala-sql` migration test asserts `vala.drift_alerts` absent; `EVAL_PROFILE_UID` removed; `vala-core` removed from CI codegen packages; `policy_hook` removed from the pools allowlist; new journeys registered in `mise.toml`; `test:operators:integration` and release-gated `test:operators:smoke:live` lanes added | `codegen:check`, `check:from-pools-allowlist`, every lane below | PASS |
| REQ-114 architecture and public docs | `architecture/wyrd-design.md` PagerDuty routing; Policy invoke-gate prose removed; shipped Verifier/Trigger/Operator docs; Bifrost verification table schema; Operator connection configuration, CLI, authorization, and how-to pages | `docs:check`, `check:skills-sync` | PASS |

Lanes run sequentially in this session, all exit 0: `fmt`, `py:format`,
`codegen:check`, `check:client-tier`, `check:pyo3-scope`,
`check:tenant-isolation`, `check:registry-tx-coupling`,
`check:from-pools-allowlist`, `check:unwrap-audit`, `lints`, `py:lints`,
`py:typecheck`, `ts:typecheck`, `ts:napi:check`, `test:cards:integration`,
`test:principals:integration`, `test:sql`, `test:shared`, `test:vala`,
`test:wyrd`, `test:wyrd-sdk`, `verify:bifrost`, `test:wyrdstate:journey`,
`test:platform:journey`, `test:cli:journey`, `test:bifrost:journey:sdk`,
`test:bifrost:journey:server`, `test:bifrost:journey:mcp`,
`test:bifrost:journey:python`, `test:bifrost:journey:typescript`,
`test:operators:integration`, `py:test:unit`, `py:test:integration`,
`ts:test:unit`, `ts:test:integration`, `docs:check`, `check:skills-sync`,
`git diff --check`. Closeout: `mise run gate` exit 0 before the decode fix; after it `test:sql`, `test:cards:integration`, `fmt`, and `lints` exit 0, and focused `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-sql --test pg_cards_register -E 'test(=pending_operation_with_null_stored_response_decodes)'` passes (run with `CARGO_BUILD_JOBS=8` after a host low-memory kill of the first attempt).

Diagnosis (`test:wyrd`):
- **Symptom:** `wyrd-spec vala::trace::attributes_tests::wyrd_keys_count_locked` failed with `left: 8, right: 9`.
- **Evidence:** `crates/wyrd-spec/src/vala/trace/mod.rs:408`.
- **Cause:** this task deliberately removed the retired `EVAL_PROFILE_UID` key from `WYRD_KEYS`, but the count lock still expected the old size.
- **Fix site:** the lock now expects 8; nothing else references the removed key.
- **Focused check:** `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=vala::trace::attributes_tests::wyrd_keys_count_locked)'` passes.

Limits recorded for change review:
- **Stale-writer fence.** AC-025 is proven through a stand-in stale table config, because no schema-evolution API exists.
- **Ceilings proven only indirectly.** The 16-binding global ceiling and the real 30s Operator timeout are proven only through the existing per-tenant saturation test and the defaults unit test.
- **Not run.** Credentialed live smokes (`test:operators:smoke:live`) and the image journeys remain release-gated and were not run here.
