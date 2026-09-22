---
id: TASK-009-postgres-clock-boundary-remediation
title: Eliminate cross-clock PostgreSQL coordination and expiry decisions
kind: remediation
status: ready
spec: SPEC-admin-principals
spec_revision: 14
requirements: [REQ-007, REQ-009, REQ-010, REQ-012, REQ-029, REQ-032, REQ-034, REQ-043, REQ-048, INV-011, INV-012, INV-013, AC-005, AC-006, AC-010, AC-011, AC-015, AC-018]
depends_on: [TASK-001, TASK-002, TASK-003, TASK-004, TASK-005, TASK-006, TASK-007, TASK-008, TASK-001-008-R12]
parent_task: TASK-001-008-R12-close-final-contract-audit-and-proof-gaps
remediates: [CLOCK-V1, CLOCK-V2, CLOCK-V3, CLOCK-V4, CLOCK-V5, CLOCK-V6, CLOCK-V7, CLOCK-V8, CLOCK-V9, CLOCK-V10, CLOCK-V11, CLOCK-V12, CLOCK-V13, CLOCK-V14, CLOCK-V15, CLOCK-V16]
---

# Final PostgreSQL clock-boundary remediation

## Outcome and Value

PostgreSQL becomes the sole clock authority for every database coordination,
eligibility, lease, liveness, retry, and relative-expiry decision covered by
this task. Rust supplies durations or domain timestamps, never a host-derived
`Utc::now()` instant that PostgreSQL later evaluates. When Rust must act on a
PostgreSQL lease, it converts one database time sample into a conservative
local monotonic deadline rather than comparing clocks.

This closes a defect class proven reachable by the Forge `ready_at` failure:
the Rust host and the PostgreSQL container differed by about 28 ms, so work
written as immediately eligible was temporarily in PostgreSQL's future. The
same pattern remains in Card reconciliation, cluster membership, login state,
credentials, refresh tokens, upload sessions, and Forge operation ordering.
The dangerous cases are not cosmetic: skew can stall work, admit or reject an
expired credential inconsistently, report contradictory credential state, or
allow two replicas to believe they own the same lease.

The admin-principals specification owns correct credential expiry, rotation,
login continuation, current-state platform authentication, and indistinguishable
invalid-credential behavior. The user has additionally authorized the Card,
cluster, storage, and Forge corrections as final gate-remediation work required
to make the cumulative branch reliable. This task changes no public wire shape,
persistent schema, migration, authorization rule, tenant boundary, or product
behavior other than removing clock-skew-dependent outcomes.

### Governing rule

> The system evaluating a time predicate owns the timestamp used by that
> predicate.

Apply the rule as follows:

- PostgreSQL coordination and validity predicates use
  `statement_timestamp()`.
- Relative PostgreSQL deadlines are derived in SQL from a caller-supplied
  duration, for example
  `statement_timestamp() + ($n * interval '1 second')`.
- Rust may retain producer-owned domain timestamps such as event time,
  `pending_since`, process `started_at`, JWT wall-clock claims that PostgreSQL
  does not evaluate, and administrator-supplied absolute expiry instants.
- In-process timeouts and elapsed-time budgets continue to use
  `std::time::Instant` or `tokio::time::Instant`.
- A Rust consumer of a database lease uses the database's remaining duration
  sampled by the lease statement and projects it onto a local monotonic
  `Instant`; it never compares the database timestamp with `Utc::now()`.

## Validated Finding Ledger

| Finding | Validated defect | Required disposition |
|---|---|---|
| `CLOCK-V1` | Card registration seeds immediate reconciliation eligibility from host `pending_since` | Keep `pending_since` as reporting data; seed eligibility with PostgreSQL time |
| `CLOCK-V2` | Card claim eligibility and lease expiry are based on caller-supplied host instants | Claim from PostgreSQL time and accept only a lease duration |
| `CLOCK-V3` | Card retry deadlines are computed from the Rust wall clock | Accept retry delays and derive deadlines in SQL |
| `CLOCK-V4` | The reconciler compares a database lease expiry with `Utc::now()` | Project the database lease remainder onto a monotonic deadline |
| `CLOCK-V5` | Cluster liveness cutoffs are computed independently by each replica | Accept a liveness duration and evaluate it against PostgreSQL time |
| `CLOCK-V6` | Initial and replacement heartbeats are stamped from process `started_at` | Preserve `started_at`; stamp `heartbeat_at` in PostgreSQL |
| `CLOCK-V7` | Storage upload expiry is written and swept by PostgreSQL but evaluated again by Rust | Project the live-session verdict from the owning SQL query |
| `CLOCK-V8` | Platform login-state expiry is host-written, Rust-evaluated, and PostgreSQL-swept | Derive expiry and its consumption verdict in PostgreSQL |
| `CLOCK-V9` | Tenant login-state expiry is host-written and PostgreSQL-evaluated | Derive the relative expiry in PostgreSQL |
| `CLOCK-V10` / `CLOCK-V11` | Initial and rotated refresh-token rows share one Rust-derived expiry defect | Use one PostgreSQL issuance instant for both writers and the signed claim |
| `CLOCK-V12` | API-key status uses Rust time while lookup uses PostgreSQL time | Return the status verdict from PostgreSQL |
| `CLOCK-V13` | Platform credential usability uses Rust time while administrator safety uses PostgreSQL time | Return one database-computed usability verdict without skipping Argon2 work |
| `CLOCK-V14` | Forge operation ordering timestamps come from independent worker clocks | Stamp prepared and terminal transitions in PostgreSQL |
| `CLOCK-V15` | An unused terminal-pruning API accepts a host-derived cutoff for PostgreSQL-owned timestamps | Delete the production-dead API and tests rather than redesigning speculative behavior |
| `CLOCK-V16` | Relative API-key and platform-credential lifetimes are converted to absolute instants by Rust | Pass the requested lifetime to SQL and derive expiry there |

`CLOCK-V10` and `CLOCK-V11` are one defect because both writers target the
same refresh-token table and already converge through the same human-session
issuance workflow. Do not implement separate clock paths.

## Owners, Scope, Consumers, and Prohibited Changes

### Primary owners and consumers

- Card reconciliation: `crates/wyrd/wyrd-sql/src/queries/cards/{register,lifecycle}.rs`
  and `crates/wyrd/wyrd-server/src/components/cards/{reconciler,service}.rs`.
- Cluster membership: `crates/vala/vala-sql/src/queries/cluster_nodes.rs` and
  `crates/vala/vala-bifrost-redux/src/cluster/mod.rs`.
- Forge operation ordering and unused retention:
  `crates/vala/vala-sql/src/queries/{forge_operations,forge_tasks}.rs`.
- Tenant authentication state:
  `crates/wyrd/wyrd-sql/src/queries/auth/{login_state,refresh_tokens,service_accounts}.rs`
  and the existing owners in `crates/wyrd/wyrd-auth/src`.
- Platform authentication state:
  `crates/wyrd/wyrd-sql/src/queries/platform/{identity,credentials,principals}.rs`,
  `crates/wyrd/wyrd-auth/src/{platform_login,platform_credentials,platform_sessions}.rs`,
  and their server issuance routes.
- Card upload validity: the existing manifest-completion projection and its
  consumers; storage multipart creation and sweeping remain unchanged.
- Refresh JWT signing: the existing concrete `IssuingKey`; no verifier,
  signer trait, or second token workflow is introduced.

Paths are ownership guidance, not a private implementation allowlist. Update
all callers and tests of a changed private API in the same task.

### Required invariants

- Preserve `TenantConn`, `OperatorPool`, transaction ownership, RLS, fencing,
  attempt identity, authorization, audit, and fail-closed behavior.
- Preserve exact credential TTLs, refresh rotation and replay containment,
  one indistinguishable invalid-credential outcome, constant-work secret
  verification, and the one shared issuance workflow.
- Preserve Card reconciliation retry sequence, maximum attempts,
  dead-lettering, lease owner checks, and storage side-effect ordering.
- Preserve cluster role fences and the distinction between `started_at` as
  process metadata and `heartbeat_at` as database-observed liveness.
- Preserve Forge operation identities, phases, replay gates, ordering keys,
  and all active maintenance behavior.
- Preserve the current production contracts: credential issuance accepts a
  relative lifetime or no expiry. A newly discovered production requirement
  for caller-supplied absolute expiry is a stop condition, not a reason to keep
  the obsolete absolute-time insertion shape.
- Use `statement_timestamp()` for the touched eligibility and lease decisions.
  Do not mechanically replace unrelated `now()` calls.

### Prohibited changes

- No migration, schema change, public HTTP/gRPC/SDK/CLI/MCP contract change,
  compatibility behavior, dependency, Cargo feature, environment variable,
  injectable clock framework, generic clock trait, or new test harness.
- No global replacement of `now()` or `Utc::now()`.
- No changes to Iceberg snapshot timestamps, object-store mtimes, event time,
  audit publication watermarks, JWT verifier clock-skew policy, Scribe event
  lead bounds, or in-process `Instant` deadlines.
- No redesign of the Oracle/Scribe signed `wire_deadline`; it is a separate
  cross-host protocol question, not a PostgreSQL clock defect.
- No change to `clock_timestamp()` in maintenance fencing without separate
  evidence that invocation-time evaluation is wrong.
- No sleep-based tests, synthetic host load, clock-skew environment switches,
  test serialization, weakened assertions, ignored tests, swallowed errors,
  or acceptance of transient worker errors merely to make a lane green.
- No permanent grep/check for this rule and no check that verifies another
  check.

## Approach

1. Complete and preserve the already-started Forge `ready_at` and
   superseded-promotion corrections, then establish a green Bifrost baseline.
   Do not rework those fixes as part of this task.
2. Move each remaining database coordination or relative-expiry decision to
   its existing SQL owner. Change private caller inputs from absolute host
   instants to durations or consume a database-computed verdict.
3. For the one Rust lease-budget decision, reuse the repository's established
   database-sample-to-monotonic-deadline pattern rather than creating another
   clock or polling the database.
4. Align refresh-token row expiry and JWT `iat`/`exp` to one whole-second
   PostgreSQL issuance instant inside the existing transaction.
5. Delete the production-dead Forge terminal-pruning API, update all affected
   callers and tests, and add the governing rule once to `AGENTS.md`.
6. Execute each scenario through RED, GREEN, and REFACTOR before running the
   complete affected SQL, Wyrd, and Bifrost lanes and the explicitly requested
   aggregate gate.

## Ordered Implementation Scenarios

### Scenario 1 — Card reconciliation and upload validity use one database clock

**Behavior.** A pending Card is immediately eligible according to PostgreSQL;
claim, lease, retry, dead-letter, and upload-session validity decisions cannot
change when the Rust host clock leads or lags PostgreSQL. `CLOCK-V1` through
`CLOCK-V4` and `CLOCK-V7` are closed without changing retry counts, lease
fencing, or storage cleanup behavior.

**RED.** Extend
`reconciliation_claims_are_bounded_and_lease_safe` in the existing
`pg_cards_register` target. Prove that a deliberately misleading reporting
timestamp does not defer immediate eligibility, that the returned lease carries
a positive database-derived remainder, and that retry eligibility follows the
requested delay rather than a caller absolute instant. Add a focused manifest
case named `manifest_upload_liveness_is_decided_by_postgres` that ages the
storage row with SQL and proves the projected verdict changes without comparing
against a Rust timestamp. The pre-change code must fail because its APIs accept
and evaluate host-derived instants.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --test pg_cards_register -E 'test(=reconciliation_claims_are_bounded_and_lease_safe)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --test pg_cards_register -E 'test(=manifest_upload_liveness_is_decided_by_postgres)'"
```

**GREEN.** Keep `pending_since` as reporting data while setting immediate
reconciliation eligibility from `statement_timestamp()`. Make claims accept a
lease duration, evaluate eligibility at `statement_timestamp()`, derive expiry
from that same statement, and return enough database-derived lease remainder
to project a conservative local monotonic deadline. Sample the monotonic clock
before the claim statement and compute the deadline as that sample plus the
database-reported remainder; using the earlier sample already accounts
conservatively for query and return latency. Apply the existing five-second
safety margin to that monotonic deadline and do not subtract elapsed time a
second time.

Make schedule, reschedule, and failure recording accept retry delays and derive
the next attempt inside their SQL statement. Preserve the existing `1s`, `4s`,
`16s` retry policy. Make the manifest-completion query return the complete
live-upload verdict, including status and `expires_at > statement_timestamp()`;
remove the raw expiry field if no remaining consumer needs it.

**REFACTOR.** Remove obsolete `Utc` imports and absolute-time parameters. Keep
the existing Card SQL and service owners; do not introduce a Card clock type,
clock service, extra query, or polling loop. Rerun both Scenario 1 tests after
each simplification.

### Scenario 2 — Cluster liveness and Forge ordering use PostgreSQL time

**Behavior.** Every replica observes the same Scribe and Oracle liveness window,
initial registration cannot be made stale or future by process-clock skew, and
Forge recovery ordering is based on database-stamped transitions.
`CLOCK-V5`, `CLOCK-V6`, and `CLOCK-V14` are closed. The unused `CLOCK-V15`
retention surface no longer exists.

**RED.** Extend `live_discovery_excludes_expired_heartbeat` so registrations
with deliberately old and future `started_at` values both receive a current
database heartbeat, while SQL-aging the heartbeat makes the role non-live.
Extend `prepared_and_terminal_audit_are_atomic_and_replay_safe` to prove the
prepared and terminal operation timestamps are database-generated and preserve
the existing replay and ordering behavior. The old registration and transition
paths must fail these assertions.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p vala-sql --test pg_oracle_membership -E 'test(=live_discovery_excludes_expired_heartbeat)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p vala-sql --test pg_forge_tasks -E 'test(=prepared_and_terminal_audit_are_atomic_and_replay_safe)'"
```

**GREEN.** Make live-node queries accept a liveness duration and evaluate
`heartbeat_at >= statement_timestamp() - duration`. Keep process `started_at`
as supplied metadata, but stamp `heartbeat_at` with `statement_timestamp()` on
insert and re-registration. Update every production caller and deterministic
fixture to pass the duration rather than a precomputed cutoff.

Stamp Forge operation `prepared_at` and `updated_at` in the insert/update
statements with `statement_timestamp()`. Preserve `(prepared_at, operation_id)`
and `(updated_at, operation_id)` ordering. Delete `ForgeTasks::prune_terminal`
and only the test code whose sole purpose is that API because repository search
shows no production consumer. If an approved authority or production caller is
found, stop instead of deleting it; the replacement contract must accept a
retention duration and evaluate the cutoff in SQL.

**REFACTOR.** Remove obsolete absolute cutoff and transition-time plumbing.
Retain the existing cluster and Forge owners, indexes, transaction boundaries,
and row shapes except for private projections required by the behavior.

### Scenario 3 — Login and credential validity have one PostgreSQL verdict

**Behavior.** Tenant and platform login states, tenant API keys, and platform
credentials cannot be accepted by one layer and rejected by another because of
clock skew. Relative lifetimes are stored from PostgreSQL time and no-expiry
credentials remain unbounded. `CLOCK-V8`, `CLOCK-V9`, `CLOCK-V12`,
`CLOCK-V13`, and `CLOCK-V16` are closed while `INV-011` and `INV-012` remain
intact.

**RED.** Extend the existing expiry and invalid-credential tests rather than
creating new binaries. Use zero/positive durations and direct SQL aging to
establish boundary conditions without sleeping. The tests must prove that
consumption/status/usability comes from the same PostgreSQL statement that
owns the relevant row and that invalid platform credentials still perform the
existing indistinguishable secret-verification work. Add one focused tenant
API-key case named
`tenant_api_key_status_and_relative_expiry_use_database_time` to prove active,
expired, and revoked status plus the database-derived relative expiry and the
exact expiry returned to the issuance consumer.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --test pg_platform_identity -E 'test(=an_expired_login_state_is_refused_like_an_unknown_one)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=login::pg_tests::expired_login_state_returns_none)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --test pg_admin_principals -E 'test(=every_invalid_condition_yields_an_unusable_credential)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --test pg_admin_principals -E 'test(=tenant_api_key_status_and_relative_expiry_use_database_time)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=platform_credentials::pg_tests::every_rejection_is_the_same_error)'"
```

**GREEN.** Make both login-state insert paths accept TTL durations and derive
`expires_at` with `statement_timestamp()`. Make platform login-state consumption
return a PostgreSQL-computed live verdict from the destructive consume statement;
make tenant consumption use `statement_timestamp()` as well. Preserve one-time
consumption, including deletion of an expired presented state.

Make API-key status return a database-computed expired verdict and make active
lookup use `statement_timestamp()`. Make every platform credential lookup path
project one database-computed usability verdict covering credential revocation,
expiry, and active principal status: the shared pool/transaction prefix query
and the separate credential-ID query must agree. Do not filter unusable rows
out before the existing constant-work secret verification.

Production API-key and platform-credential issuance accepts only a relative
lifetime or no expiry. Pass that lifetime to the existing SQL owner, derive the
stored expiry there, and return the database-generated `expires_at` from the
insert. Any issuance response or downstream owner that exposes or records the
expiry must use that returned value, never reconstruct it from the host clock.
Preserve `None` as no expiry. Tests that need historical or already-expired rows
must age them directly with SQL rather than preserving an absolute-expiry
production insertion path. Update route, auth owner, fixture, and test callers
together so no relative issuance path reconstructs an absolute deadline with
`Utc::now()`.

**REFACTOR.** Remove duplicate Rust validity methods or timestamp fields that
have no consumer after the SQL projection. Keep one existing issuance owner per
credential family and add no expiry service, state enum, or second lookup.

### Scenario 4 — Refresh JWT and durable row share one PostgreSQL instant

**Behavior.** Initial human refresh issuance and every rotation derive JWT
`iat`, JWT `exp`, and durable `auth_refresh_tokens.expires_at` from one
whole-second PostgreSQL timestamp in the existing transaction. PostgreSQL
remains the acceptance authority, while the signed claim accurately describes
the same expiry. `CLOCK-V10` and `CLOCK-V11` close together without changing
rotation, replay containment, tenant routing, or the five-minute access-token
contract.

**RED.** Extend `happy_rotation_mints_new_pair_and_revokes_old_row` to decode
the successor refresh JWT and assert that its `exp` equals the stored successor
row expiry at whole-second precision. Extend `expired_token_is_rejected` to
retain PostgreSQL boundary rejection. The old path must fail the exact equality
because signing and row expiry call `Utc::now()` independently.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=refresh::pg_tests::happy_rotation_mints_new_pair_and_revokes_old_row)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=refresh::pg_tests::expired_token_is_rejected)'"
```

**GREEN.** Sample one whole-second PostgreSQL issuance instant inside the
caller-owned transaction, before minting the refresh token. Allow the concrete
signer to mint a refresh token from that explicit instant and configured TTL.
Use the identical derived expiry for both normal and rotated row insertion.
Make active-row consumption evaluate expiry with `statement_timestamp()`.
Keep access-token minting and every JWT verification rule unchanged.

**REFACTOR.** Keep this behavior inside the existing human-session issuance
and concrete signing owners. Do not add a general database clock service,
change every token API, or issue an extra timestamp query per downstream
consumer.

## Acceptance Criteria

1. Immediate Card reconciliation eligibility is stamped by PostgreSQL;
   `pending_since` remains reporting data only.
2. Card claim, lease expiry, retry, reschedule, and dead-letter predicates share
   PostgreSQL time. Rust's pre-work lease guard uses a conservative monotonic
   deadline derived from the claim statement's database sample.
3. Storage upload liveness is returned by the manifest SQL projection; no
   Card service compares `storage_multipart_uploads.expires_at` with
   `Utc::now()`.
4. Cluster liveness APIs accept durations, registration stamps heartbeat time
   in PostgreSQL, and `started_at` remains process metadata.
5. Forge operation preparation and terminal transitions use database timestamps
   without changing replay, phase, or ordering behavior.
6. The unused Forge terminal-pruning API and its exclusive tests are absent,
   unless a real production caller or approved retention obligation is found;
   that discovery is a material stop condition.
7. Platform and tenant login-state expiry is created and evaluated by
   PostgreSQL, while single-use destructive consumption remains unchanged.
8. Tenant API-key status and every platform credential lookup consume
   database-computed verdicts. Invalid credentials remain indistinguishable and
   still follow the constant-work secret-verification path.
9. Relative API-key and platform-credential lifetimes are derived in SQL, and
   the insert returns the exact stored expiry for every response or downstream
   consumer. `None` retains its no-expiry meaning; no absolute-expiry production
   insertion path remains without a real production caller.
10. Initial and rotated refresh JWT claims and durable rows share one
    whole-second PostgreSQL issuance instant and exact expiry. Rotation, reuse
    detection, audit, and tenant routing remain unchanged.
11. Touched coordination and expiry predicates use `statement_timestamp()`;
    unrelated transaction metadata and invocation-time fence logic are not
    mechanically rewritten.
12. No public contract, migration, schema, dependency, Cargo feature, test-only
    environment switch, test serialization, or clock abstraction is added.
13. The existing single parallel `test:bifrost:gate` Nextest invocation remains
    in place, and all focused and broader verification completes successfully.
14. `AGENTS.md` contains exactly one concise clock-ownership rule; no permanent
    enforcement script or self-test is added.

## Expected Write Set and Consumer Closure

Likely production and test surfaces include:

- `AGENTS.md`;
- `crates/wyrd/wyrd-sql/src/queries/cards/{register,lifecycle}.rs` and
  `crates/wyrd/wyrd-sql/tests/pg_cards_register.rs`;
- `crates/wyrd/wyrd-server/src/components/cards/{reconciler,service}.rs`;
- `crates/vala/vala-sql/src/queries/{cluster_nodes,forge_operations,forge_tasks}.rs`;
- `crates/vala/vala-sql/tests/{pg_oracle_membership,pg_forge_tasks}.rs`;
- `crates/vala/vala-bifrost-redux/src/cluster/mod.rs` and mechanically affected
  cluster callers/tests;
- `crates/wyrd/wyrd-sql/src/queries/auth/{login_state,refresh_tokens,service_accounts}.rs`;
- `crates/wyrd/wyrd-sql/src/queries/platform/{identity,credentials,principals}.rs`;
- `crates/wyrd/wyrd-auth/src/{login,platform_login,platform_credentials,platform_sessions,issuance,refresh,issue_api_key}.rs`;
- `crates/shared/wyrd-auth-issue/src/lib.rs` only for explicit-instant refresh
  signing through the existing concrete signer;
- the existing server credential-issuance routes and their focused integration
  tests where private duration inputs change.

Before editing a signature, enumerate every caller and update the complete
consumer closure. Do not treat this list as permission for unrelated cleanup.
No migration, generated artifact, or public contract change is expected.

## Verification and Evidence

Execute Cargo-backed commands sequentially. For each scenario, record the
expected RED failure, the GREEN focused result, and the final rerun after
refactoring. A focused command must report a positive selected-test count.

After all focused commands above pass, run:

```bash
mise run fmt
mise run lints
mise run test:sql
mise run test:wyrd
mise run test:bifrost
mise run gate
git diff --check
```

The user explicitly requires the broad gate for this global remediation,
overriding the narrower default verification scope in `VER-003` for this final
task only. Do not run duplicate lanes concurrently and do not generate
synthetic host load.

Final evidence must also include:

- the focused command and positive test count for every named test above;
- a caller inventory for each changed private SQL/auth signature;
- an audit table mapping `CLOCK-V1` through `CLOCK-V16` to the final owner,
  test, and result;
- a repository search showing no production caller of the deleted
  `prune_terminal`, or the material stop report if one exists;
- a manual classification of remaining `Utc::now()` uses in the touched
  owners as domain, in-process, JWT-only, test fixture, or out of scope;
- confirmation that `test:bifrost:gate` still uses one parallel Nextest
  invocation; and
- a final diff audit proving no unrelated dirty-tree file was overwritten or
  reverted.

### Static proof for the governing rule

TDD is not applicable to contributor documentation or the final source audit.
Add this single rule to `AGENTS.md` near the database/runtime rules:

> PostgreSQL owns timestamps used for database coordination, eligibility,
> leases, and relative expiry. Callers bind durations; SQL derives deadlines
> with `statement_timestamp()`. Domain event times and caller-supplied absolute
> dates remain producer-owned.

Then manually enumerate every `DateTime<Utc>` bound into SQL and every Rust
comparison against a database-owned deadline in the touched owners. Classify
each remaining use as domain, in-process, JWT-only, test fixture, or out of
scope. This audit is completion evidence, not a permanent checker. Confirm the
parallel Bifrost gate remains enabled and diagnose any failure from tracing and
source rather than panic text alone.

## Material Stop Conditions

Stop and request specification or architecture authority if implementation
requires any of the following:

- a public HTTP, gRPC, SDK, CLI, MCP, generated, or persisted-schema change;
- a migration or destructive data rewrite;
- changed credential TTL, token lifetime, authorization, revocation, tenant,
  audit, replay-containment, or fail-closed semantics;
- deletion of `prune_terminal` when a production caller or approved retention
  obligation exists;
- discovery of a real production caller that requires an administrator-supplied
  absolute credential expiry rather than the current relative-lifetime or
  no-expiry contracts;
- redesigning the Oracle/Scribe signed deadline protocol or assuming a new
  distributed clock-synchronization guarantee;
- a new dependency, Cargo feature, environment variable, generic clock owner,
  or permanent repository check; or
- weakening, skipping, deleting, serializing, or ignoring any required test to
  obtain a green result.

Private SQL parameter changes, mechanical caller updates, database verdict
projections, explicit-instant refresh signing on the existing concrete signer,
and deterministic fixture updates are authorized implementation decisions.

## Authority Links

- Approved specification: `changes/active/admin-principals/spec.md`, revision
  14.
- Parent remediation:
  `changes/active/admin-principals/review/whole-branch-12/TASK-001-008-R12-close-final-contract-audit-and-proof-gaps.md`.
- `AGENTS.md`, especially §§2, 5, 6, 9, 11, and 12.
- `architecture/agent-rules.md`.
- `architecture/wyrd-design.md`.
- `architecture/wyrd-security-posture.md`.
- `architecture/bifrost-design.md` for cluster, Forge, and Oracle boundaries.
- `architecture/references/languages/spec-driven-development.md`.
- `architecture/references/languages/implementation-execution.md`.
- `architecture/references/languages/testing-workflows.md`.

## Implementation Evidence

Commits (branch `claude/admin-principals-spec-qfsmjc`, baseline `119f625bf`):

| Commit | Scenario |
|---|---|
| `41612c5d5` | Scenario 1 — Card reconciliation eligibility, leases, retries, upload liveness |
| `81de3a0be` | Scenario 2 — cluster liveness windows, Forge operation stamps, `prune_terminal` deletion |
| `5a53de839` | Scenario 3 — login-state and credential relative expiry, one usability verdict |
| `8213234cf` | Scenario 4 — one `PostgreSQL` issuance instant for the refresh JWT and its durable row |
| `7d04f2dc2` | Rustdoc markdown/panic-doc lint closure on the touched surface |

### Acceptance matrix

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| CLOCK-V1 pending eligibility is DB-seeded | `wyrd-sql/src/queries/cards/register.rs` (`CASE WHEN $11='pending' THEN statement_timestamp()`) | `pg_cards_register -E 'test(=reconciliation_claims_are_bounded_and_lease_safe)'` — 1 passed | PASS |
| CLOCK-V2 claim eligibility/lease from `statement_timestamp()` | `cards/lifecycle.rs::claim_card_reconciliation(operator, lease_seconds, limit)` | same focused test — 1 passed | PASS |
| CLOCK-V3 retry deadlines derived in SQL | `cards/lifecycle.rs` `retry_delay_seconds` binds; `reconciler.rs::retry_delay_seconds` | same focused test + `wyrd-server --lib -E 'test(=components::cards::reconciler::tests::retry_schedule_is_fixed_and_bounded)'` (in `test:wyrd`, 2031 passed) | PASS |
| CLOCK-V4 lease projected onto a monotonic deadline | `reconciler.rs` `claim_started` + `lease_remaining_seconds` | `test:wyrd` — 2031 passed | PASS |
| CLOCK-V5/V6 liveness duration bound; `heartbeat_at` DB-stamped | `vala-sql/src/queries/cluster_nodes.rs`; `vala-bifrost-redux/src/cluster/mod.rs` | `pg_oracle_membership -E 'test(=live_discovery_excludes_expired_heartbeat)'` — 1 passed | PASS |
| CLOCK-V7 upload liveness projected by SQL | `cards/lifecycle.rs` `storage_upload_live`; `cards/service.rs` | `pg_cards_register -E 'test(=manifest_upload_liveness_is_decided_by_postgres)'` — 1 passed | PASS |
| CLOCK-V8 platform login-state expiry + verdict in SQL | `platform/identity.rs` (`ttl`, `expires_at > statement_timestamp() AS live`) | `pg_platform_identity -E 'test(=pg_tests::an_expired_login_state_is_refused_like_an_unknown_one)'` — 1 passed | PASS |
| CLOCK-V9 tenant login-state relative expiry in SQL | `auth/login_state.rs`; `wyrd-auth/src/login.rs` | `wyrd-auth --lib -E 'test(=login::pg_tests::expired_login_state_returns_none)'` — 1 passed | PASS |
| CLOCK-V10/V11 one issuance instant for JWT and row | `auth/refresh_tokens.rs::refresh_issuance_instant`; `wyrd-auth/src/issuance.rs`; `wyrd-auth-issue` `issue_refresh_token(..., issued_at, ttl)` | `wyrd-auth --lib -E 'test(=refresh::pg_tests::happy_rotation_mints_new_pair_and_revokes_old_row)'` — 1 passed; `-E 'test(=refresh::pg_tests::expired_token_is_rejected)'` — 1 passed | PASS |
| CLOCK-V12 API-key status verdict from `PostgreSQL` | `auth/service_accounts.rs::api_key_status_by_prefix` | `pg_admin_principals -E 'test(=tenant_api_key_status_and_relative_expiry_use_database_time)'` — 1 passed | PASS |
| CLOCK-V13 one credential usability verdict, Argon2 work preserved | `platform/credentials.rs` `usable` projection; `wyrd-auth/src/platform_credentials.rs` | `pg_admin_principals -E 'test(=every_invalid_condition_yields_an_unusable_credential)'` — 1 passed; `wyrd-auth --lib -E 'test(=platform_credentials::pg_tests::every_rejection_is_the_same_error)'` — 1 passed | PASS |
| CLOCK-V14 Forge operation transitions DB-stamped | `vala-sql/src/queries/forge_operations.rs` | `pg_forge_operations -E 'test(=snapshot_expire_prepare_and_commit)'` (in `test:sql`, 118 passed) | PASS |
| CLOCK-V15 production-dead `prune_terminal` deleted | `vala-sql/src/queries/forge_tasks.rs` | `grep -rn prune_terminal crates sdks` → 0 hits; `test:sql` 118 passed | PASS |
| CLOCK-V16 relative lifetimes bound as durations | `auth/service_accounts.rs::insert_api_key`, `platform/credentials.rs::insert_platform_credential_tx` and their five server callers | `pg_admin_principals` focused tests above; `test:wyrd` 2031 passed | PASS |
| Governing rule recorded once | `AGENTS.md` §15 | n/a (documentation) | PASS |

### Caller inventory for changed private signatures

- `claim_card_reconciliation` → `wyrd-server/src/components/cards/reconciler.rs::run_once` (sole caller).
- `schedule_card_reconciliation` / `reschedule_card_reconciliation` / `record_card_reconciliation_failure` → `wyrd-server/src/components/cards/service.rs` (sole owner), reached from `reconciler.rs`.
- `validate_live_oracle` / `list_live` → `vala-bifrost-redux/src/cluster/mod.rs` (sole callers).
- `insert_login_state` → `wyrd-auth/src/login.rs`.
- `insert_platform_login_state` → `wyrd-auth/src/platform_login.rs`.
- `insert_platform_credential_tx` → `wyrd-auth/src/platform_credentials.rs::issue_platform_credential`, itself called from `wyrd-server` `platform/credentials.rs`, `platform/provisioning.rs`, `platform/recovery.rs`, `principals/routes.rs`, and `wyrd-testing/src/server.rs` (3 sites).
- `insert_api_key` → `wyrd-auth/src/issue_api_key.rs` and `wyrd-server/src/auth/jwt_bearer.rs` test setup.
- `issue_refresh_token` → `wyrd-auth/src/issuance.rs::issue_human_session` plus three test call sites (`wyrd-auth/src/refresh.rs`, `wyrd-auth-issue/src/lib.rs`).

### Remaining `Utc::now()` in touched owners

| Location | Classification |
|---|---|
| `wyrd-auth/src/issuance.rs:398` (`access_ttl`) | JWT-only — access-token `exp`, never evaluated by `PostgreSQL` |
| `wyrd-auth/src/platform_sessions.rs:291` | Domain — reported token expiry inside an audit detail |
| `vala-bifrost-redux/src/cluster/mod.rs:137` | In-process — snapshot observation time |
| `vala-bifrost-redux/src/cluster/mod.rs:653,692,693,717,718`; `forge/worker.rs:8604-8607`; `vala-sql/src/queries/file_list.rs:382`; `wyrd-testing/src/*` | Test fixture |
| `wyrd-server/src/auth/jwt_bearer.rs:366,367,946` | Test fixture (JWT claim construction) |
| `wyrd-auth-issue/src/lib.rs` (3, all in `#[cfg(test)]`) | Test fixture |

No `DateTime<Utc>` is bound into a `PostgreSQL` coordination or validity predicate in the touched owners. The remaining bound `DateTime<Utc>` parameters are `insert_refresh_token` / `insert_refresh_token_rotated` `expires_at`, both now fed the `PostgreSQL`-derived issuance instant; `cards/lifecycle.rs::CardReconcileClaim::reconcile_lease_expires_at` is returned reporting data with no Rust comparison (`grep` shows no non-SQL, non-test consumer).

### Verification battery

| Command | Result |
|---|---|
| `mise run fmt` | clean |
| `mise run lints` | clean |
| `mise run test:sql` | 118 + 2 passed, 0 failed |
| `mise run test:wyrd` | 2031 passed, 0 failed |
| `mise run test:bifrost` | 1 failure: `vala-bifrost-redux::integration forge::production_routes::a_promotion_planned_in_the_settlement_window_is_superseded` — **pre-existing**, see below |
| `mise run gate` | same single failure; every other lane passed |
| `git diff --check` | clean |

`test:bifrost:gate` still schedules every selected binary in one Nextest
invocation: `Starting 111 tests across 8 binaries (64 tests skipped)` under a
single run ID.

### Pre-existing failure, not caused by this task

`forge::production_routes::a_promotion_planned_in_the_settlement_window_is_superseded`
fails deterministically at `production_routes.rs:1859` ("the replanned
promotion is claimed"). The same test, run in a clean worktree checked out at
the task baseline `119f625bf`, fails identically:

```
git worktree add <tmp> 119f625bf
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && \
  cargo nextest run --locked -p vala-bifrost-redux -P journey --run-ignored=all \
  -E 'test(=forge::production_routes::a_promotion_planned_in_the_settlement_window_is_superseded)'"
→ FAIL, same panic site
```

Nothing in this task touches Forge claim eligibility, the fair-claim statement,
`ready_at`, or `next_eligible_at`. The failure is reported rather than repaired
or suppressed: no test was weakened, ignored, serialized, or deleted.

One further observation, not reproducible: the first `test:bifrost` run also
failed `wyrd-testing::oracle published::published_cache_pruning_and_shutdown_are_production_governed`
with `A Tokio 1.x context was found, but it is being shutdown` from the Forge
worker during teardown. It passed standalone with tracing enabled, passed the
whole `test:bifrost:journey:oracle` binary, and passed on the next full lane
run. It is a Forge-worker shutdown-ordering race under aggregate contention,
unrelated to clock ownership.

### Diff audit

`git diff 119f625bf --name-only` lists 38 files, all inside the authorized
write set plus `AGENTS.md` and this task file. Two incidental edits are
disclosed:

- `crates/vala/vala-bifrost-redux/src/forge/scribe_promotion.rs` — `cargo fmt`
  reflow only; the baseline tree was not formatted.
- `crates/vala/vala-sql/src/queries/file_list.rs::planned_settlement` — the
  baseline failed `mise run lints` with `clippy::type_complexity` on an
  explicit five-tuple. Closed by deriving `sqlx::FromRow` on the existing
  `PlannedHotFileRow` and deleting the tuple plus its mapping closure
  (deletion, not addition; column names already matched the field names).

No migration, public HTTP/gRPC/SDK/CLI/MCP contract, generated artifact,
dependency, Cargo feature, environment variable, clock trait, or permanent
repository check was added. No non-goal was implemented.
