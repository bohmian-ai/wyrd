---
id: TASK-001-R3
kind: remediation
status: approved
spec: SPEC-surfaces-oracle-integration
spec_revision: 7
requirements: [REQ-026, INV-025, AC-005]
depends_on: [TASK-001-R2]
parent_task: TASK-001
remediates:
  - FIND-TASK-001-R1-5
  - FIND-TASK-001-R1-6
  - FIND-TASK-001-R1-7
  - FIND-TASK-001-R2-1
---

# Close TASK-001-R2 review findings

Route directly to `$wyrd-implement`. Recommendations are the validated `ponytail-rev` ledger recorded in
`verdict.md` (this directory).

## Authority and immutable subjects

- Approved spec: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-001-integrate-redux-data-plane.md`
- Prior remediation: `review/task-001-r1-8e61c0349-review-01/TASK-001-R2-close-r1-review-findings.md`
- Reviewed candidate: `0ca4ee7ef7416f870ce3244add406901b57337de`; R2 base `8e61c0349`; R1 base `8377fff9f`
- Review: `verdict.md`, `task-review.md`, `standards-review.md`, `domain-review-security-tenancy-durability.md` in this directory

## Issue diagnoses and required corrections

### 1. Prove the publication concurrency ceiling (`FIND-TASK-001-R1-5`)

- **Obligation:** R2 criterion "production audit sweep ... never exceeds the fixed concurrency bound"; INV-025 forbids accepted gaps.
- **Current behavior:** the bound constant (`crates/wyrd/wyrd-server/src/audit/publication.rs:55`, used at `:164`) has no executable proof. R2 deleted the disconnected unit test; the only journey (`a_stalled_tenant_does_not_block_another_tenants_history`) uses two tenants. The journey doc (`crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:296-298`) says the ceiling is unproven, and the R2 evidence (`TASK-001-R2...md:276`) calls this "human-approved", but no approval is recorded anywhere.
- **Why the stated blocker is wrong:** the publisher uses the Vala pool (default 16, `WYRD_DB_MAX_CONNECTIONS_VALA`); the tenant directory uses `OperatorPool`; `WYRD_DB_MAX_CONNECTIONS` (defaulted to 8 by `with-test-postgres.sh`, overridable) only caps the app pool the test's locks would come from; each `WyrdTestServer` has its own ephemeral database.
- **Consequence:** changing the constant or making the sweep unbounded passes every test.
- **Reviewer's proposed correction:** a 9-tenant sibling journey holding 8 chain-head locks, asserting the 9th tenant is not retained until release.
- **Human decision (Steven Forrester, 2026-09-13): not required.** The ceiling proof is rejected as overkill. The value 8 is a resource bound, not a correctness property: gapless chains, frozen-range replay, and batch-id dedup hold at any concurrency, and the existing two-tenant journey proves a stalled tenant does not block another. The constant stays unproven by test, and the journey doc says so honestly. No code change for this item. A future change to the bound needs no new test unless it alters publication semantics.

### 2. Make required journey lanes deterministic (`FIND-TASK-001-R1-6` a)

- **Obligation:** TASK-001 / R2 cumulative verification requires `test:bifrost:journey:mcp` and `:server` green; INV-025.
- **Current behavior:** R2 records `journey:server` 2/3 and `journey:mcp` 2/4 and attributes them to TASK-006, which records no such ownership. Validator reproduced `journey:mcp` 7/8: `query::pg_tests::delegated_agent_query_is_attributed_in_its_durable_audit_record` got `RowNotFound`. The test's `read_decision_detail` (`crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs:258-274`) reads `vala.audit_staging`, which the server's own publisher drains at boot and every 5s.
- **Consequence:** the test races the production publisher and asserts "durable" audit from transient state.
- **Correction:** have `read_decision_detail` poll retained `audit.audit_log` for `detail` by operation and request_id through the existing `ScheduledQueryCaller` read, following the `retained_rows` / `await_retained` pattern (`wyrd-mcp` already dev-depends on `wyrd-server` with `test-support`). No fallback to staging. For `journey:server`, change code only if a failure reproduces during proof; then diagnose that specific test at its root.

### 3. Record TASK-002 lane transfer (`FIND-TASK-001-R1-6` b)

- **Obligation:** R2 non-goal "Do not broaden into TASK-002's Postgres launcher repair".
- **Current behavior:** commit `662bf33bc` rewrote TASK-002's five Postgres lanes and `:inner` tasks in `mise.toml` and edited `scripts/postgres/check-inventory.py:31-32`. R2's own focused commands did not need it (the wrapper and `db:migrate:all:inner` existed at base).
- **Consequence:** TASK-002's acceptance appears pre-met by unreviewed shared infrastructure.
- **Correction:** do not revert (reverting restores tasks depending on the deleted `setup:db-roles`, and TASK-002 prescribes the same lines). In `changes/active/surfaces-oracle-integration/tasks/TASK-002-converge-client-and-sdks.md`, under "Postgres Test Lifecycle Correction", record that the change landed at `662bf33bc` and remains unverified until TASK-002 runs its five lanes and `test:postgres:inventory`. Remove any claim in TASK-001 remediation records that R2 delivered it.

### 4. Finish touched Rust contracts (`FIND-TASK-001-R1-7`)

- (a) `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:1489-1490`: new `append_registration_audit` takes `&mut wyrd_sql::TenantConn<'_>`, violating the bare-type rule and Vala-tier import through `vala_sql`. Add `TenantConn` to the existing `use vala_sql::{...}` and use `conn: &mut TenantConn<'_>`. Leave pre-R2 qualified uses (`:1507`, `scribe_promotion.rs:443`) alone.
- (b) `crates/wyrd/wyrd-server/src/components/cards/routes.rs`: `register_card_http`, `delete_card_http`, `delete_card_by_ref_http` changed audit behavior in R2 without rustdoc for it or `# Errors`. Document where the verdict is written (inside the service transaction; standalone when register lacks an idempotency key) and add `# Errors` (RBAC denial, `AuditUnavailable`, invalid idempotency key/kind/UID/ref, service failures).

### 5. Record service-account verdicts on every exit (`FIND-TASK-001-R2-1`)

- **Obligation:** REQ-026 (every permitted authorization decision audited exactly once); R2's own module doc at `crates/wyrd/wyrd-server/src/components/admin/routes.rs:15`.
- **Current behavior:** these handlers append the Allowed row inside their tenant transaction and return early on error, rolling the row back with nothing re-recording it:
  - workload-binding create `admin/routes.rs:403-408` (duplicate 409, unknown issuer 404)
  - trusted-issuer delete without cascade `admin/routes.rs:336-351` (live binding 409)
  - workload-binding delete `admin/routes.rs:473-478`
  - principal revoke `crates/wyrd/wyrd-server/src/auth/revoke.rs:47-52` (unknown principal)
  - API-key issuance `crates/wyrd/wyrd-server/src/components/auth/routes.rs:294-301` (any failure)
  - any connection failure after the verdict on these paths
- **Consequence:** a `service_accounts:write` holder can probe credential-admin state without a durable record; the module doc is false.
- **Correction:** reuse the R2 Card pattern. Move `record_unless_committed` unchanged from `crates/wyrd/wyrd-server/src/components/cards/service.rs:1784` into `crate::audit` and update Card callers. In each of the five handlers, wrap post-verdict work (connection acquisition through commit) in one `Result` passed to it, so an uncommitted outcome records the same Allowed event once via `audit::record_audit`, keeping `AuditUnavailable` fail-closed. The two delete routes' not-found branches that already commit must return as committed outcomes and map to 404 after the helper, so they do not record twice.

## Constraints and preserved behavior

- Preserve everything closed in R2 (R1-1..R1-4, R1-8, R1-9), Redux as sole engine, RLS, gapless chain, frozen-range publication, Scribe fences, Forge lineage, denial semantics, stable error codes, SSRF pinning, secret redaction.
- No new compatibility surface, migration, scheduler, configuration, dependency, public test seam, harness, or test file.
- Non-goals: reverting `662bf33bc`; verifying TASK-002 lanes; fixing pre-R2 qualified type uses; SDK convergence; `verify:bifrost`; standards observations O-1..O-4.

## Acceptance criteria

| Criterion | Finding |
|---|---|
| Human decision to leave the concurrency bound unproven is recorded in this task (§1) | R1-5 |
| MCP delegated-query audit test asserts retained `audit.audit_log`, not staging; `journey:mcp` and `journey:server` green three consecutive runs | R1-6 (a) |
| TASK-002 records `662bf33bc` as landed and unverified; no TASK-001 record claims it | R1-6 (b) |
| `append_registration_audit` uses bare `TenantConn` via `vala_sql`; the three Card handlers have audit rustdoc and `# Errors` | R1-7 |
| Each listed service-account error path records exactly one `allowed` row and has no effect; committed paths still record once; module doc true | R2-1 |

## Focused proof

Confirm exact names with `mise exec -- cargo nextest list`. Run Postgres tests one at a time.

1. `WYRD_REG_E2E=1 scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=audit_publication::a_stalled_tenant_does_not_block_another_tenants_history)'"` (no ceiling case; see §1 decision).
2. `mise run test:bifrost:journey:mcp` and `mise run test:bifrost:journey:server`, three consecutive runs each, with counts.
3. `git grep 662bf33bc -- changes/active/surfaces-oracle-integration/tasks/TASK-002-converge-client-and-sdks.md`; `python3 scripts/postgres/check-inventory.py`.
4. `mise exec -- cargo clippy --locked -p vala-bifrost-redux --all-targets --all-features -- -D warnings`.
5. Extended `components::admin::routes::pg_tests::create_binding_for_unknown_issuer_is_not_found`, `components::admin::routes::pg_tests::delete_issuer_with_live_binding_conflicts_then_cascades`, and the existing revoke-not-found test, each asserting one `allowed` row via `decision_rows` and no effect, through exact `-p wyrd-server --lib --features test-support -E 'test(=...)'` under `with-test-postgres.sh`; re-run R2 focused Card set (`pg_card_registration_route` 8 tests) for the helper move.

Then the cumulative set from TASK-001-R2 "Focused proof" (fmt, lints, codegen:check, test:sql, bifrost integration/journey lanes, isolation/pool/pin/unwrap/client-tier/pyo3 checks, docs:check, `git diff --check`).

## Implementation evidence

Human direction (Steven Forrester, 2026-09-13) replaced §2 and §5 corrections and the
three-consecutive-runs proof: one audit write path (`vala.audit_staging`) and one
`AuditPublisher`; the Oracle audit WAL and relay are deleted; each lane runs once.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| R1-5 decision recorded | §1 (`4de23e201`) | n/a | PASS |
| R1-6 (a) MCP journey deterministic | Audit assertions removed from the MCP delegated query journey; chain attribution stays in the server query journey (`a5089326f`). Oracle WAL deleted; Oracle commits to staging from a tracked task (`dfc930031`) | `test:bifrost:journey:mcp` 8/8; `test:bifrost:journey:server` 9/9; `test:bifrost:journey:oracle` 28/28 | PASS |
| R1-6 (b) TASK-002 transfer note | `a41adeafc` | n/a | PASS |
| R1-7 bare `TenantConn`, Card handler docs | `a41adeafc`; OpenAPI regenerated | `mise run codegen:check` pass | PASS |
| R2-1 service-account verdicts survive failure | Allowed row committed standalone before the transaction in trusted-issuer delete, binding create/delete, principal revoke, API-key issue (`4272ef988`) | wyrd-server lib admin/auth/revoke pg tests 22/22 | PASS |
| Single audit writer | Removed wyrd-sql Card-delete audit row, tracing eval audit writer, `vala_sql` `record_audit` alias (`dfc930031`) | `pg_eval_v1_protocol` 9/9; `pg_cards_register` `exact_delete_enforces_inbound_references_and_is_idempotent`; `test:bifrost:integration:server` 67/67 | PASS |
| Auth audit on the single outbox | Token exchange, API-key issue, refresh-family revocation, card-scope mint and auth failures append `AuditEvent`s to `vala.audit_staging`; wyrd-sql auth audit tables, inserts and migrations deleted (`de2a6a7fa`, `6d18cbfce`) | wyrd-auth and wyrd-server auth pg tests read staging; `mise run lints` pass | PASS |
| Python journey green | Stale client expectations corrected (`WyrdError`, `{"variant": ...}` details); Scribe recovery decodes system-owner staged records, admitted only for the audit log (`scribe/hot_stage.rs`) | `test:bifrost:journey:python` 34/34; `scribe::hot_stage::tests::a_system_owner_record_decodes_and_is_limited_to_the_audit_log`; redux clippy pass | PASS |

Also: `mise run lints` pass; `mise run fmt` clean; `git diff --check` clean.

Limits: Oracle read-audit commit failures are logged and counted, not replayed.
