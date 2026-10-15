# Persistent data, concurrency, and durability review

Result: **FAIL**

Subject: base `b22718f2cf5da0ebcb7f26bc376157e5eec7768f` through candidate
`518a5d8b822a4e649a5fe90b14f3eb2b1fc8a0aa`; HEAD still matched the candidate
at the end of source inspection. Approved authority is spec revision 4 and
TASK-001. No implementation files were changed.

## Boundary and authority coverage

| Boundary | Authority | Source and proof inspected | Assessment |
|---|---|---|---|
| Creator pairs and immutable updates | REQ-005/009, INV-001/002/007; AGENTS server ownership and SQL capability rules | Defining auth/Card/operator/Bifrost migrations; `cards/register.rs`, `cards/auth_projection.rs`, `auth/service_accounts.rs`, `operator_connections.rs`, `olap_catalog.rs`; `pg_cards_register::subject_actor_creation_and_replay` | Ordinary insert/update paths preserve creator pairs; bootstrap exceptions below do not satisfy approved rules |
| Pair-scoped replay and concurrency | REQ-009, AC-014; TenantConn/RLS authority | Registration operation unique index, insert/lookup/replay, RegistrationWriter transaction; manual run advisory lock, keyed lookup, defining unique index, start_run authorization-before-replay; pair replay tests and existing scheduler concurrency tests | Pair included consistently in lookup, insertion, uniqueness and manual serialization; no authority update on replay; existing owners extended |
| Accepted authority persistence and recovery | REQ-008/011, INV-003/006/007, AC-013/017 | `AcceptedAuthority`, `RunAuthority`, `INSERT_RUN_SQL`, observation batch insert, `CLAIM_RUN_SQL`, `ClaimedRunRow::into_claimed`, runner, Eval judge caller; V3 source | Snapshot credential ID stripped at capture; JSON frozen on same run; claim/retry/release/settle do not replace it; V3 proof limitation below |
| Background acceptance | REQ-008, AC-001/002/017 | BackgroundAuthority::resolve, scheduler claim/settlement, ObservationRunSink, lock_observation_bindings and enqueue_observation_runs | TenantConn-bound owner resolution reused through issuer recovery; writer grants no authority; observation missing-owner visibility remains a limit noted below |
| Managed Arrow/Iceberg identity | REQ-005/006, AC-005/016; Bifrost authority; OLAP/Iceberg/reliability reference slices | Gate AuthContext/dispatch, ScribeIngressFrame, ingress/preprocess/native decode, execution_lanes managed arrays, managed_columns, claim_sample, catalog registration, defining schemas, ingest tests | Both identities stamped from admitted context; schema policy controls stamping, audit identity remains content; subject replaces writer stratum with same sampler algorithm; existing catalog lock/WAL/registered field-ID owners retained |
| Schema rollout | Expensive-to-reverse decisions; AC-007 | Full migration name-status diff and defining SQL changes | No added migration; requester-every-origin migration deleted; cause context folded into defining run CREATE TABLE; existing idempotency migration retains its distinct purpose |

Read the task-review skill, common subject/navigation map, approved spec, task
and recorded execution evidence; agent rules, spec-driven-development and
maintainer references; reference router and applicable OLAP, Iceberg and
analytical reliability slices; relevant identity, authority, managed-column
and provisioning passages of design/security/Bifrost authority. CodeGraph was
used before targeted source navigation. This is a domain review, not a claim
of full SDK, Rustdoc, or repository-standards coverage.

## Proposed findings

### DATA-001 — SYSTEM is persisted as its own creation subject

- Classification: **INCORRECT**.
- Obligation: REQ-003 “SYSTEM is never a sub”, INV-002; REQ-005 includes
  `wyrd.auth_service_accounts` in the creation-attribution inventory.
- Exact changed location:
  `crates/wyrd/wyrd-sql/migrations/20260601000028_system_principal.sql:79`.
- Producer-to-consumer evidence: `wyrd.provision_system_principal()` inserts
  `candidate` as the row ID, `sub_principal_id`, and `act_principal_id`, while
  declaring `principal_kind='system'`. The shared SQL wrapper at
  `queries/auth/service_accounts.rs:254` supplies no creator context.
  Production `TenantProvisioning::establish_tenant_administration` calls it
  at `components/platform/provisioning.rs:456` despite already receiving the
  verified platform creation `attribution`. The same function is called by
  development seeding (`shared/wyrd-dev-fixtures/src/pg.rs:684`), test-server
  setup, and the migration backfill. Its conflict branch correctly preserves
  the first row, so the incorrect subject remains durable on every retry.
- Observable consequence: every freshly provisioned SYSTEM principal has a
  work-attribution record saying SYSTEM was the beneficiary and creator of
  its own creation. This is an actual persisted counterexample to the
  approved invariant, not merely stale prose or an ID/owner-key exception.
- Smallest correction boundary: extend the existing provisioning owner to
  supply its trusted creation attribution to the existing SYSTEM insert,
  just as it already does for tenant-admin and credential creation. Preserve
  one-per-tenant identity and conflict/no-update behavior. Reconcile existing
  callerless bootstrap/backfill callers explicitly with approved authority;
  do not replace SYSTEM with a new fabricated identity or silently add an
  exception. If callerless bootstrap needs an exception, that requires a
  specification decision rather than a local convention.
- Focused closure proof: provision a real tenant, inspect the SYSTEM row's
  pair alongside the admin/key creator pair, assert SYSTEM is not its
  creation subject, then provision again and assert the original ID and pair
  remain unchanged. Extend existing provisioning/SQL fixture tests; no new
  store or test harness is needed.

### DATA-002 — Built-in table creation expands the unresolved-audit sentinel contract

- Classification: **DRIFT / INCORRECT**.
- Obligation: INV-001 permits the explicit unresolved-**audit** exception in
  REQ-010; REQ-005 includes `vala.bifrost_tables` in the creator-pair inventory.
  REQ-010 grants no general platform-internal creation exception.
- Exact changed locations:
  `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:931` and
  `crates/wyrd-spec/src/auth/attribution.rs:38`.
- Producer-to-consumer evidence: `ensure_builtin` unconditionally constructs
  `Attribution::unresolved()`. `create_table_locked` persists that pair through
  `olap_catalog::upsert_table`, so all built-in catalog rows carry the fixed
  `PLATFORM_AUDIT_PRINCIPAL` as both creator IDs. New Attribution rustdoc and
  the defining OLAP migration explicitly broaden this sentinel to internal
  provisioning. `establish_and_promote` already has verified platform creator
  attribution but drops it before `BuiltinTables::ensure_tenant`
  (`components/platform/provisioning.rs:278–288`). Its sibling startup path
  `BuiltinTables::reconcile` genuinely has no authenticated caller and may
  create missing built-ins. User dataset registration correctly passes its
  actual creator through the same catalog/SQL owner.
- Observable consequence: even a table created as part of an authenticated
  tenant provisioning operation permanently loses its real creator pair and
  instead records an unresolved authentication marker. Reconciliation
  installs an additional persistent-data identity convention absent from the
  approved spec. This is unrelated to the transport sentinel that correctly
  remains separate from audit payload identity.
- Smallest correction boundary: reuse catalog registration's existing
  attribution input to retain trusted provisioning creator identity where it
  exists. Remove the unapproved generalization of the audit sentinel.
  Callerless startup creation needs an explicit approved attribution rule;
  do not invent one in remediation, change eager provisioning availability,
  or mint a SYSTEM token. This part is an authority decision if no existing
  approved rule can be identified.
- Focused closure proof: authenticate tenant provisioning with a known
  platform principal, read its built-in catalog rows and verify the approved
  creation pair; repeat ensure/reconcile and prove creator immutability.
  Separately prove the approved callerless-startup case once its rule is
  established, using existing BuiltinTables/catalog integration fixtures.

### DATA-003 — Restart proof never exercises the captured authority at a protected consumer

- Classification: **MISSING** verification of required behavior.
- Obligation: AC-013 requires that a later subject-only lookup cannot authorize
  the denied judge model after reclaim/restart; Scenario 3 requires every
  judge to check model permission after recovery.
- Exact location:
  `crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs:2916`
  (`queued_subject_authority_survives_restart`, EngineScript setup).
- Evidence: V3 accepts an actual delegated manual Drift run and changes
  grants. Both recreated runtimes then use `EngineScript` to return Retry or
  Completed without executing a Verifier or gateway authorization. The test
  compares the stored JSON before/after and checks the result pair, which
  proves persistence but cannot fail if execution replaces the loaded
  principal with fresh subject grants. Eval's production recovered path
  consumes `run.authority` through `Caller::accepted` at `verification/eval.rs:205`;
  that source looks consistent, but V3 never reaches this consumer. Manual
  Drift has no judge, and no expanded public manual-Eval contract is needed.
- Observable consequence: the task's central negative recovery condition is
  claimed PASS while its proof bypasses the permission decision it must
  protect. Realtime V1 does not cover the serialized/reloaded authority seam.
- Smallest correction boundary: retain V3's queue lifecycle proof and add a
  focused owning integration test that loads accepted delegated authority on
  a fresh claim/context and passes it through the existing protected gateway
  caller. Subject permits the model, captured delegated authority excludes
  it, and new grants would permit it; the recovered call must still refuse
  with no provider request. Reuse the existing queue and gateway fixtures;
  do not add queued manual Eval, a second authority store, a fake permission
  evaluator, or a process-crash harness.
- Focused closure proof: the exact new test must fail when the recovered
  caller is rebuilt from fresh subject authority and pass with the frozen
  principal; retain the JSON round-trip/result-pair proof.

## Limits and recovery assessment

- No Cargo/build/runtime checks were run during this independent static
  review; V1–V11 and static/owner results are recorded implementation evidence.
- Existing PostgreSQL uniqueness, binding row locks, SKIP LOCKED leases,
  lease fencing, and immutable run input/authority fields remain the shared
  mechanisms. No second durable queue or snapshot store was introduced.
- Ordinary entity updates and run claim/retry/release/settlement SQL contain
  no assignment to either creator pair or accepted authority. Scribe keeps
  its existing admitted-batch/WAL/field-ID flow; this change does not alter
  acknowledged-write durability.
- Missing observation owner rows are silently skipped in
  `ObservationRunSink::write` before authority resolution; the orchestrator
  was notified for resilience/observable-failure coverage. This report does
  not independently propose an additional finding without resolving the
  complete binding-status consumer contract.
- No optional schema hardening, new migrations/backfill, extra revocation,
  general framework, or broad verification lane is requested.

Skipped runtime execution and unrelated language/style coverage; the retained
risks are the persisted bootstrap identity exceptions and the recovery proof gap.
