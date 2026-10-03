---
id: AUDIT-OUTBOX-T02
title: One process-wide batched audit outbox on every surface
kind: implementation
status: ready
spec: SPEC-audit-outbox
spec_revision: 1
depends_on: [AUDIT-OUTBOX-T01]
requirements: [REQ-001, REQ-002, REQ-003, REQ-005, REQ-007, INV-001, INV-002, INV-003, INV-004]
acceptance: [AC-001, AC-002, AC-003, AC-007]
---

# One process-wide batched audit outbox on every surface

## Outcome and Value

Every audited surface (Gate writes, `start_run`, Cards, operators, principals,
admin, platform, storage, OTLP, query, Oracle, peer authority, gateway, Bifrost
catalog registration, and `wyrd-auth` grants) stages its decision on the one
process outbox with a non-blocking enqueue. No request waits on or fails
because of an audit commit. The writer commits tenants concurrently on a
bounded number of connections, and shutdown drains the queue to a deadline.

## Owners, Scope, Consumers, and Prohibited Changes

- Owner of the outbox type: the crate that owns the canonical staging append
  (`vala-sql`), so `wyrd-auth`, Gate/Oracle composition, and `wyrd-server`
  share one concrete type without `wyrd-auth` depending on `wyrd-server`. The
  server constructs exactly one instance per process (`AppState::audit_outbox`)
  and hands it to Oracle, Gate, gateway, verification, and auth callers.
- The staging append stops being a production entry point outside the outbox
  (AC-001), proven by visibility or a test.
- Gate's audit trait becomes non-blocking (no `Result`), like Oracle's.
- Prohibited: changing event content, hash chaining, or the publisher.

## Approach

1. Generalize `OracleQueryAudit` into the shared outbox: concurrent bounded
   tenant commits, one failure counter labelled by surface, shutdown drain.
2. Construct one instance at boot; route Oracle, Gate, verification, gateway
   invocation (drop the per-event task), and every `audit::*` caller to it.
3. Convert `wyrd-auth` appenders and the Bifrost catalog registration append
   to staging after the decision; platform authorization stages and still
   returns its operator transaction, without an audit row in it.
4. `start_run`: stage the decision; never hold the chain head across enqueue.

## Ordered Implementation Scenarios

### Scenario 1 — Gate write and run start succeed with audit failing

**Behavior.** With inserts into `vala.audit_staging` revoked, a Gate write and a
`start_run` succeed and `audit_outbox_commit_failures_total` increments (AC-002,
REQ-003).

**RED.** A server journey revoking the staging insert, then writing and
starting a run; today both are refused with audit-unavailable.

**GREEN.** Route Gate and verification through the outbox.

**REFACTOR.** Delete `PostgresGateAudit` and the synchronous helpers.

### Scenario 2 — Concurrent same-tenant decisions from two outboxes stay gap-free

**Behavior.** Two outbox instances (two replicas) staging for one tenant
commit a gap-free, ordered chain (AC-003, INV-002).

**RED.** A Postgres integration test with two outboxes; it does not compile
until the outbox exists in its shared owner.

**GREEN.** Shared outbox with concurrent tenant commits.

**REFACTOR.** Keep one writer type.

### Scenario 3 — Shutdown drains queued events

**Behavior.** Events queued before shutdown commit within the deadline (AC-007).

**RED.** The same integration target stages events and calls shutdown; it
does not compile until the shared outbox exists.

**GREEN.** Shutdown closes the queue and waits for the writer to the deadline.

**REFACTOR.** None beyond the shared writer.

## Acceptance Criteria

- No production code outside the outbox writer calls the staging append.
- The surface families named in AC-002 succeed with audit failing (isolated
  tests where an end-to-end journey is not the owning tier).
- Two outboxes for one tenant produce a gap-free ordered chain.

## Expected Write Set and Consumer Closure

`crates/vala/vala-sql`,
`crates/wyrd/wyrd-server/src/{audit,oracle,bifrost,components,query,http,auth,boot,state.rs,app}`,
`crates/wyrd/wyrd-auth/src`, `crates/vala/vala-bifrost-redux/src/{gate,oracle,catalog}`,
`crates/wyrd/wyrd-testing`.

## Verification and Evidence

`mise run fmt`, `mise run lints`, `mise run test:wyrd`,
`mise run test:principals:integration`, `mise run test:bifrost:journey:server`,
`mise run test:bifrost:integration:redux`, `mise run test:bifrost:integration:sql`.

## Material Stop Conditions

A surface whose correctness requires the audit row to be atomic with its effect.

## Authority Links

`changes/active/audit-outbox/spec.md`, `AGENTS.md` §2-§6.

## Implementation Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| No production code outside the outbox writer calls the staging append (AC-001, REQ-001) | `crates/vala/vala-sql/src/audit_outbox.rs` (`AuditOutbox`, the only caller of the crate-private `append_audit_events`); `append_audit`/`append_audit_batch` in `queries/audit_staging.rs` are `#[cfg(feature = "test-support")]`; `PostgresGateAudit` (`bifrost/gate_audit.rs`) and `oracle/query_audit.rs` deleted | `cargo check --workspace --all-targets --all-features` and `mise run lints` compile every production crate without `test-support` | PASS |
| One process outbox on every surface (REQ-002, INV-001) | `boot/mod.rs` builds one `AuditOutbox`, `AppState::audit_outbox` hands it to Oracle, Gate, gateway, verification, Cards, operators, principals, admin, platform, storage, OTLP, query, peer authority, Bifrost catalog registration and `wyrd-auth` issuers; shutdown drains it | `mise run test:wyrd`, `mise run test:principals:integration`, `mise run test:bifrost:journey:server` | PASS |
| Gate write and `start_run` succeed with audit failing; failures counted (AC-002, REQ-003) | Gate audit trait is non-blocking; `verification` stages before the effect | journey `audit_publication::a_gate_write_and_a_run_start_succeed_while_audit_commits_fail` (trigger rejects every staging insert; `bifrost` and `verification` surface counters increment; the row is readable) | PASS |
| Other AC-002 surface families succeed with audit failing (isolated tests) | stage-then-act on each surface | Cards: `pg_card_registration_route::{registration,completion,delete}_succeeds_when_its_decision_audit_fails`; admin: `admin::routes::pg_tests::an_unrecordable_{issuer,binding}_create_still_writes_its_*`; platform: `platform_admin_e2e::an_unrecordable_tenant_mutation_still_happens`, `platform_authz::pg_tests::an_unrecordable_decision_still_authorizes`, `platform_sessions::pg_tests::an_unrecordable_grant_still_returns_the_session`; auth: `exchange_api_key::pg_tests::a_refused_exchange_audit_still_issues_the_token`, `pg_openapi_contract::an_unstageable_exchange_audit_still_grants_a_token`; gateway: `gateway_invocation_dispatches_without_waiting_for_the_audit_append`, `gateway_failed_operations_keep_one_allowed_decision_and_never_wait_on_audit`; Oracle peer: `oracle::peer_audit::tests::oracle_peer_postgres_audit_routes_security_identity_and_detail` | PASS |
| Two outboxes for one tenant commit a gap-free ordered chain (AC-003, INV-002) | concurrent per-tenant commits in the shared writer, one commit in flight per tenant | `vala-sql` `pg_audit_outbox::two_outboxes_commit_one_gap_free_chain_and_drain_on_shutdown`; `a_contended_tenant_does_not_delay_another_tenants_audit` | PASS |
| Shutdown drains queued events to a deadline (AC-007) | `AuditOutbox::shutdown` cancels intake and waits for the writer | `two_outboxes_commit_one_gap_free_chain_and_drain_on_shutdown` | PASS |

Tests inverted or removed because the behavior they pinned is now forbidden
(requests never fail on audit):

- `pg_openapi_contract::an_unavailable_audit_store_answers_with_a_code_the_operation_documents`
  removed; the exchange variant became `an_unstageable_exchange_audit_still_grants_a_token`.
- SDK "describe audit fault" journeys (Rust `observe_run`, Python
  `test_observe_journey`, TypeScript `observe-run.test.ts`) and their testing
  hooks (`sdks/wyrd-sdk-python/src/testing.rs`, `sdks/wyrd-sdk-ts/native-testing`,
  stubs) removed: they asserted a refusal that no longer exists. The surviving
  proof is the AC-002 journey above.
- Admin, auth-route and gateway tests that asserted "no allowance when the
  write failed" now assert the allowance staged before the effect.

### Diagnosis: 60 s teardown stalls in Postgres test lanes

- **Symptom:** in `test:principals:integration` and `test:wyrd`, whole groups of
  `wyrd-auth` and `wyrd-server` Postgres tests finished together after ~62 s
  (`SLOW [> 60.000s]`). Under load the same window coincided with
  `pg_operator_delivery::verifiers_progress_while_operator_deliveries_are_capped`
  and `wyrd-cli operator_journey` failures that pass in isolation.
- **Evidence:** a `--test-threads 1` run pinned single culprits
  (`admin::routes::pg_tests::workload_binding_crud_round_trip`, five gateway
  administration tests, `gateway_budget_reservations_cover_attempts_in_exact_periods`,
  `gateway_managed_credentials_resolve_per_tenant_across_restart_and_rotation`)
  at 61 s each while every other test took ~1 s.
- **Cause:** decisions are staged before their effect, so the outbox's commit
  task begins opening a pool connection while the effect runs. When the test's
  last request returns, `PgFixture`'s synchronous `Drop` blocks the
  current-thread runtime, stranding that connection mid-SCRAM. Its backend is
  not yet in the database's procarray, so `DROP DATABASE ... WITH (FORCE)`
  cannot terminate it and waits on its ProcSignalBarrier until Postgres
  `authentication_timeout` (60 s), stalling every concurrent test's DROP.
- **Fix site:** the test owners that stage audit. `wyrd-auth` tests now share
  one outbox per fixture (`issuer`, `exchange_service`, `delegate_service`,
  `refresh_service` take the test's outbox) and drain it before the fixture
  drops; admin and gateway-administration tests settle through a
  `settle_audit` helper at the end of each test; gateway invocation tests and
  `race_admissions` drain every replica they built. Other callers checked:
  `oracle::peer_audit` tests shut their outbox down or never stage; journeys go
  through `WyrdTestServer`, whose shutdown drains the process outbox.
- **Diagnostician report:** an independent read-only diagnostician, given the
  failing command, trace and diff, reported the same cause and fix site and
  noted `authentication_timeout` as an optional backstop; the backstop was not
  applied because the per-owner drain removes the stranded login.

Commands:

- `mise run test:wyrd` — 2327/2327 (65 s; no test above 60 s after the teardown fix)
- `mise run test:principals:integration` — all binaries green (admin module 15/15 in 4.2 s, was 62 s)
- `mise run test:bifrost:journey:server` — 30/30 (includes the AC-002 journey)
- `mise run test:bifrost:journey:mcp` — 14/14
- `mise run test:bifrost:journey:sdk` — 17/17 (`wyrd-client` `pg_bifrost_e2e`)
- `mise run test:gateway:native` — 9/9 Rust gateway journeys incl. resilience, plus the Python native gateway journeys
- `mise run test:bifrost:integration:redux` — 873/873
- `mise run test:bifrost:integration:sql` — 117/117 (includes `pg_audit_outbox`)
- Focused, with `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && ...'`:
  `cargo nextest run --locked -p wyrd-server --lib --test-threads 1 -E 'test(/admin::routes::pg_tests|pg_invocation_tests|pg_administration_tests/)'`
  and `-p wyrd-auth --lib -E 'test(/pg_tests/)'` — no test over 11 s
- `mise run codegen:check`, `mise run py:test:unit` (529 passed), `mise run py:typecheck`,
  `mise run py:format`, `mise run py:lints`, `mise run fmt`, `mise run lints`, `git diff --check`

Non-goals untouched: event content, hash chaining, the publisher. The
`AGENTS.md`, architecture and docs edits in the working tree belong to T03 and
are committed there.

IMPLEMENTED
