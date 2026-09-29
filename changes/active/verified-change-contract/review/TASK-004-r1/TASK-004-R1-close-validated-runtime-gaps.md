---
id: TASK-004-R1
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 33
requirements: [REQ-086, REQ-087, REQ-115, REQ-145, REQ-146, INV-007, AC-023, AC-030]
depends_on: []
parent_task: TASK-004
remediates: [FIND-TASK-004-1, FIND-TASK-004-2, FIND-TASK-004-3, FIND-TASK-004-4, FIND-TASK-004-5, FIND-TASK-004-6, FIND-TASK-004-7]
---

# Close validated generic verification runtime gaps

## Subject and authority

- Approved spec: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-004-generic-verification-runtime-and-results.md`
- Reviewed base: `9431906eeb1c7b67a0efcec09487fd1d848f70a8`
- Reviewed candidate: `49ad24707de47378b9df51764034c49a129c6b8b`
- Validated ledger: `changes/active/verified-change-contract/review/TASK-004-r1/findings-validation.md`

Implement the smallest cohesive correction for all seven validated findings.
The approved public and persisted contracts do not change.

## Issue diagnoses and required outcomes

### `FIND-TASK-004-1` — connection ownership

The runtime composition root clones the application `PgPool` into the
scheduler, publisher, runner pool wrapper, and shared verification fixture.
Those reachable owners then acquire tenant transactions themselves. This
violates the repository rule that tenant connection acquisition remains on the
existing Postgres owner and that library fields/signatures expose only the
sanctioned tenant or operator capabilities.

Delete the one-use runner pool wrapper. Give the scheduler, runner, publisher,
and fixture the narrow existing Wyrd Postgres owner already available at their
composition roots, and open tenant transactions through its `tenant_conn`
operation. Retain `OperatorPool` only for the existing authorized cross-tenant
discovery/depth reads. Callees continue to receive `&mut TenantConn<'_>` and do
not commit or roll back caller-owned transactions.

Do not introduce a connection trait, factory, callback abstraction, new pool
wrapper, or permanent source check.

### `FIND-TASK-004-2` — Arrow field identity

The summary, Drift feature, and Eval item builders currently return anonymous
ordered arrays while the finishing boundary obtains the table fields
independently and joins the two vectors by index. Adjacent fields share Arrow
types, so a valid schema reorder can silently relabel verification evidence.

Make each authored result array carry its field name. At the existing finishing
boundary, validate uniqueness and completeness, reject missing/duplicate/
unexpected authored names through the existing result-payload error surface,
and order arrays from the table owner's `arrow_fields()` by name before adding
the three named correlation fields. Keep the table definitions as the only
schema authority; do not build a generic mapping framework or duplicate them.

### `FIND-TASK-004-3` — Rust import shape

The changed SQL, runtime, fixture, MCP-test, SQL-test, and N-API modules use
fully qualified paths in fields, signatures, return types, and bounds despite
the mandatory top-of-module import rule.

Import the cited types in each existing top-level `use` block, using a narrow
collision-resolving alias only when required, and use bare names throughout the
changed fields/signatures/bounds. This is a mechanical correction: do not
reorganize modules, rename domain APIs, or change behavior.

### `FIND-TASK-004-4` — canonical SYSTEM scope decision

Gate currently audits a reserved SYSTEM write as allowed after checking only
permission, kind, and table. Scribe later accepts a missing `card_ref` column or
null rows, stamping null `card_uid`; it rejects a foreign value only after the
canonical audit already recorded allow. A live short-lived SYSTEM bearer can
therefore bypass its exact one-Verifier confinement and write unattributed
canonical result evidence.

Extend the existing native Gate/Scribe admission seam so the reserved-result
decision validates the decoded batch before the one Gate audit append. Require
exactly one UTF-8 `card_ref` field, a non-null value on every row, and successful
authorization of every parsed value against the SYSTEM principal's sole signed
UID-bearing Verifier scope. Fold this result into the existing combined
permission/kind/table decision: malformed, absent, null, or foreign values
produce exactly one denied `bifrost_record:write` event and never reach durable
admission; the exact scope produces exactly one allowed event. Preserve
Scribe's scope resolution and trusted UID stamping as defense in depth.

Do not add a permission, issuer, audit append, transport, or client-authored
tenant/Card identity.

### `FIND-TASK-004-5` — shutdown claim admission

Scheduler cancellation is observed only after a full pass, and runner
cancellation is checked before an awaited claim transaction but not across its
commit. Shutdown can therefore be followed by a new schedule cursor/run commit
or a newly leased execution, contrary to the immediate admission-close
contract.

Use the existing cancellation token at the durable transaction boundary. A
cancelled scheduler drops or rolls back the active occurrence transaction and
does not begin another tenant iteration. A cancelled runner drops or rolls back
an uncommitted claim; if claim commit wins the race, immediately use the
existing fenced release/refund transition and do not spawn execution. Preserve
schedule insert/cursor atomicity, permit-before-claim ordering, token fencing,
attempt accounting, and the 30-second drain for work spawned before
cancellation.

Do not add a shutdown table, process-local registry, or second claim protocol.

### `FIND-TASK-004-6` — role-separated identity/detail proof

The only runner-without-local-Scribe journey publishes a direct unscored Drift
summary and queries only its verdict. It cannot prove the explicit AC-023
SYSTEM/Verifier/subject/owner/binding split or detail publication.

Extend that existing journey and fixture path, without creating another
cluster or harness, to execute one non-empty binding-created Drift result.
Query summary and matching feature rows through Oracle and assert the exact
tenant SYSTEM principal, Verifier UID, subject UID, owner UID, binding ID, run
ID, result-ID join, and shared event time while preserving the proof that the
runner node owns no local Scribe.

### `FIND-TASK-004-7` — crash after detail acknowledgement

Current partial-publication coverage injects a clean pre-send summary refusal,
and current runner-crash coverage crashes before publication. Neither proves
the required crash boundary after a detail is durable while the summary
outcome is unknown. The existing `PublicationFault::hang_next` and capability
crash controls already provide the needed seam.

Use those existing controls in one real-Postgres runtime test: acknowledge a
non-empty detail batch, block the summary, crash the runner capability, inspect
the durable run and dispatch tables, then allow lease reclaim. Prove the
partial detail never completes the run or creates a dispatch, the same run
identity is reclaimed within its existing attempt budget, and completion plus
dispatch happens only after a later attempt receives every required ACK.

Do not add a repair coordinator, cross-table recovery protocol, or atomic
analytical visibility claim.

## Preserved behavior and non-goals

- Preserve the three existing Verification HTTP operations and their shared
  Rust/Python/TypeScript/MCP projections.
- Preserve the existing result schemas, SYSTEM principal/token format,
  permissions, tenant derivation, audit path, Gate/Scribe transport, Postgres
  schema, lease tokens, retry budgets, concurrency limits, and 30-second drain.
- Preserve detail-before-summary order, zero-detail behavior, sealed ambiguous
  replay, fresh-write identity, accepted partial analytical visibility, and
  dispatch only after acknowledged failed binding results.
- Preserve caller-owned `TenantConn` transaction composition and forced RLS.
- Do not implement Drift, Eval, or Operator engines assigned to later tasks.
- Do not add public APIs, dependencies, Cargo features, compatibility paths,
  brokers, local Scribe writes, process-local work registries, or permanent
  checks for mechanical source style.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-004-1` | No verification runtime or fixture field/constructor parameter carries raw `PgPool`; tenant connections come from the existing Postgres owner and all current runtime/fixture workflows still pass. |
| `FIND-TASK-004-2` | Result payload construction maps authored arrays to table fields by name, refuses missing/duplicate/unexpected names, and remains correct under an independently reordered schema. |
| `FIND-TASK-004-3` | Every cited changed Rust field/signature/bound uses a top-level imported bare type without behavioral change. |
| `FIND-TASK-004-4` | Authenticated reserved result writes with absent, null, malformed, or foreign `card_ref` are denied before durable admission with exactly one denied Gate audit event; exact Verifier scope succeeds, stamps its UID, and records exactly one allow. |
| `FIND-TASK-004-5` | Lock-controlled Postgres tests prove scheduler and runner cancellation admit no new durable work; a claim whose commit wins is fenced-released/refunded and never executed. Existing pre-cancellation drain behavior remains intact. |
| `FIND-TASK-004-6` | The existing role-separated journey proves non-empty binding-created summary/detail publication and exact SYSTEM/Verifier/subject/owner/binding/run/result/event-time identity through Oracle with no local Scribe on the runner. |
| `FIND-TASK-004-7` | A detail-ACK/summary-unknown crash leaves the run incomplete with no dispatch, reclaims the same durable run, and settles/dispatches only after all later required ACKs. |

## Focused and broader proof

Add and run exact focused commands for the new named tests after confirming
their final names with `mise exec -- cargo nextest list`. At minimum, prove:

1. name-based result mapping under reordered/missing/duplicate/unexpected
   fields;
2. authenticated gRPC exact/absent/null/malformed/foreign SYSTEM Card scope and
   audit cardinality;
3. lock-controlled scheduler and runner cancellation across claim commit;
4. the role-separated non-empty result/detail identity journey; and
5. the detail-ACK/summary-unknown crash and reclaim interleaving.

Then run the existing affected lanes sequentially:

```bash
mise run test:principals:unit
mise run test:principals:integration
mise run test:sql
mise run test:shared
mise run test:wyrd
mise run test:vala
mise run test:bifrost:integration:server
mise run test:bifrost:journey:server
mise run check:tenant-isolation
mise run check:from-pools-allowlist
mise run check:client-tier
mise run check:unwrap-audit
mise run codegen:check
mise run fmt
mise run lints
git diff --check
```

Run the existing role-separated journey through its owning `mise` lane and
retain the current publication replay/fresh-attempt, shutdown drain, crash/
restart, route, Rust/Python/TypeScript/MCP, and documentation evidence. If a
specifically named test appears in the implementation report, include its exact
focused `mise exec -- cargo nextest run --locked ... -E 'test(=...)'` command.

Route this task directly to `$wyrd-implement`. A later task review must inspect
the complete original base-to-remediated-candidate range, not only the repair
diff.

## Implementation Evidence

Commits: `f1e08e286`, `f5cc8720d`, `9b73b842c`, `c9c6f8bc4` on top of reviewed candidate `49ad24707`/`45094379f`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-004-1` | `RunnerPools` deleted; scheduler, runner, publisher, and `VerificationFixture` hold `WyrdPostgres` and open tenant work via `tenant_conn`; `OperatorPool` kept for due/runnable-tenant and depth reads (`wyrd-server/src/verification/{mod,scheduler,runner,publisher}.rs`, `wyrd-testing/src/verification.rs`) | `rg "PgPool\|TenantConn::acquire\|RunnerPools"` over runtime + fixture: no match; `test:bifrost:integration:server`, `test:bifrost:journey:server`, `test:wyrd`, `test:sql` | PASS |
| `FIND-TASK-004-2` | `results.rs`: builders emit `(field name, ArrayRef)` columns; `finish` → `assemble` orders by `T::arrow_fields()` name, refuses missing/duplicate/unexpected via `ResultPayloadError::ColumnMismatch`, then appends correlation fields | `verification::results::tests::reordered_table_fields_keep_values_bound_to_names`, `verification::results::tests::missing_duplicate_and_unexpected_columns_are_refused` | PASS |
| `FIND-TASK-004-3` | Top-level imports + bare names in `verifier_runs.rs`, `clock.rs`, `mod.rs`, `publisher.rs`, `results.rs`, `runner.rs`, `wyrd-testing/src/verification.rs`, MCP test, `pg_verifier_runs.rs`, TS native `verification.rs` | `mise run fmt`, `mise run lints`, affected lanes | PASS |
| `FIND-TASK-004-4` | `Gate::authorize_record_write` takes the native frame; the SYSTEM×result-table cell runs `require_frame_card_scope` → Scribe's `require_card_scope` (one UTF-8 `card_ref`, no nulls, `validate_card_scope`) inside the single combined decision before the one audit append; Scribe checks/stamping unchanged | `gate::tests::gate_denies_system_result_writes_outside_the_exact_card_scope`, `gate::tests::gate_confines_verification_result_tables_to_the_system_writer`, gRPC `system_result_writes_require_the_exact_signed_verifier_scope` (absent/null/malformed/foreign → PermissionDenied, audit exactly denied×4 then allowed×1, one durable row with signed UID), `system_writer_alone_writes_verification_results` | PASS |
| `FIND-TASK-004-5` | Scheduler depth/pass race `stop` (biased), dropping the uncommitted occurrence tx; runner tenant read, tx open, and claim race `stop`; a claim committed after `stop` goes through `refund_late_claim` (existing fenced `Transition::Release`) and is never spawned | `cancelled_scheduler_rolls_back_its_blocked_occurrence`, `cancelled_runner_rolls_back_its_blocked_claim`, `claim_committed_after_cancellation_is_released_unexecuted` (deferred constraint trigger + advisory lock holds COMMIT); existing drain/release tests in `pg_verification_runtime` (18/18) | PASS |
| `FIND-TASK-004-6` | Existing role-separated journey extended: binding-created two-feature failing Drift result with distinct subject/owner; fixture exposes `system_principal()` and `bind_schedule(owner, subject, …)` | `verification_runtime::runner_without_local_scribe_publishes_through_the_ingest_endpoint` asserts SYSTEM principal, Verifier UID, subject UID, owner UID, binding, run, result-ID join, shared `wyrd_event_time` through Oracle, no local Scribe | PASS |
| `FIND-TASK-004-7` | `hang_next(RESULTS)` + `CapabilityCrash` after durable detail ACK | `crash_after_detail_ack_reclaims_the_same_run_before_dispatch`: run `running`/attempt 1, no summary, 0 dispatches after crash; same run completes at attempt 2 with exactly 1 dispatch after all ACKs | PASS |

### Focused commands (all exit 0)

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::results::tests::reordered_table_fields_keep_values_bound_to_names) | test(=verification::results::tests::missing_duplicate_and_unexpected_columns_are_refused)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=gate::tests::gate_denies_system_result_writes_outside_the_exact_card_scope) | test(=gate::tests::gate_confines_verification_result_tables_to_the_system_writer)'
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test pg_grpc_ingest_smoke -E "test(=system_result_writes_require_the_exact_signed_verifier_scope) | test(=system_writer_alone_writes_verification_results)" --test-threads=1'
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test pg_verification_runtime -E "test(=cancelled_scheduler_rolls_back_its_blocked_occurrence) | test(=cancelled_runner_rolls_back_its_blocked_claim) | test(=claim_committed_after_cancellation_is_released_unexecuted) | test(=crash_after_detail_ack_reclaims_the_same_run_before_dispatch)" --test-threads=1'
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=verification_runtime::runner_without_local_scribe_publishes_through_the_ingest_endpoint)"'
```

### Lanes (all exit 0, run sequentially in this session)

`test:principals:unit`, `test:principals:integration`, `test:sql`, `test:shared`, `test:wyrd` (2120 passed), `test:vala` (1234 passed), `test:bifrost:integration:server` (78 passed), `test:bifrost:journey:server` (15 passed), `check:tenant-isolation`, `check:from-pools-allowlist`, `check:client-tier`, `check:unwrap-audit`, `codegen:check`, `fmt`, `lints`, `git diff --check`.

### Non-goals and limits

- No public API, schema, migration, permission, dependency, Cargo feature, audit append, transport, or source check added; `verifier_runs.rs` changed only in import spelling.
- Gate decodes SYSTEM result-table IPC frames once more before Scribe; bounded by `max_frame_bytes` and limited to the three result tables.
- A scheduler commit already in flight when `stop` fires may still land atomically as a pending run (never executed); documented in rustdoc.
- The lock-controlled shutdown tests hold a `SHARE` table lock rather than a row lock because both claim paths use `FOR UPDATE SKIP LOCKED`.
