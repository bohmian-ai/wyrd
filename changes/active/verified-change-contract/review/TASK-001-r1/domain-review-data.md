# TASK-001 r1 — Persistent-Data Domain Review

Reviewer: `domain-rev` (persistent data). Subject: base `5293546f3`, candidate
`2e09ae81213cb608253b75de276e3c946353ac35` (HEAD confirmed unchanged at review time).

## Reviewed Boundary

- `crates/wyrd/wyrd-sql/migrations/20260601000025_verifier_card_kind.sql` (new)
- `crates/vala/vala-sql/migrations/20260910000028_drop_drift_alerts.sql` (new)
- Deletion of `vala-sql` `queries/{alerts,drift_alerts}.rs`, `row_types/alerts.rs`,
  `tests/pg_drift_alerts.rs`, and the matching `Cargo.toml` `[[test]]` / `lib.rs`
  file-list entries
- Removal of `vala.eval.runs` / `vala.eval.assertions` from
  `crates/vala/vala-bifrost-redux/src/tables/{mod.rs,eval/}` (`BUILTIN_TABLES` 8 -> 6)
- Registration atomicity for `verified_by` validation:
  `crates/wyrd/wyrd-server/src/components/cards/{resolve.rs,service.rs}`
- Persisted Card JSON shape (`Drift`/`Eval` -> `Verifier`, `publishes_to` -> `verified_by`)

Out of scope: REQ-112 `verification_bindings` projection persistence (not in
TASK-001's requirement list; owned by a later task).

## Authority and Source Coverage

- Spec `REQ-109`: "Nothing using the old Card kinds has shipped; this change
  provides no compatibility registration or historical-card migration path."
- Spec `REQ-116`: the three tables "MUST be removed in this change ... Nothing
  using these tables has shipped, so no data migration or retained-data
  compatibility is required."
- Task Scenario 1 GREEN: "update the Card-kind persistence constraint through a
  new migration." Scenario 3: retired tables have no production or build refs.
- AGENTS.md §15 (`wyrd-sql` durable Postgres layer), §11/§12 verification.

## Checks Performed

1. **Migrations are new files, not edits.** `git diff 5293546f3 HEAD --name-status
   -- '**/migrations/**'` shows only two `A` entries. No shipped migration edited.
2. **Ordering.** `20260601000025` is the highest version in `wyrd-sql/migrations`
   (previous max `20260601000024`); `20260910000028` is the highest in
   `vala-sql/migrations` (previous max `20260910000026`). Each crate has its own
   `sqlx::migrate!("./migrations")` migrator (`wyrd-sql/src/lib.rs:70`,
   `vala-sql/src/lib.rs:61`), so cross-directory timestamps do not interleave.
   The existing `migration_filenames_match_timestamp_versions` and
   `migrations_embed_count_matches_files` tests cover monotonicity/embedding.
3. **Constraint rewrite.** Original `cards_kind_check` was explicitly named in
   `20260601000006_cards.sql:45`, so `DROP CONSTRAINT cards_kind_check` targets
   the right object. New list = old list minus `Eval`,`Drift` plus `Verifier`
   (16 values: 15 registrable + `External`), matching REQ-109's 15-kind catalog.
   No other table carries a Card-kind CHECK containing `Drift`/`Eval`
   (`card_relationships.target_kind` is unconstrained TEXT;
   `auth_service_accounts.card_kind` is Service/Agent only).
4. **Upgrade with existing `Drift`/`Eval` rows.** `ADD CONSTRAINT` would fail on a
   database holding such rows (soft-deleted included). sqlx runs each migration
   in a transaction, so the failure would be atomic, not partial. REQ-109
   explicitly declares no historical-card migration path because the kinds never
   shipped — not a finding.
5. **`DROP TABLE vala.drift_alerts`.** Created unconditionally by
   `20260821000000_add_drift_alerts.sql` in the same migrator; its only dependents
   are its RLS policy, grants, and PK index, all dropped with the table. No view,
   FK, or later migration references it. `git grep drift_alerts` outside
   `changes/` returns only the two migration files.
6. **Bifrost built-in table removal.** Built-in tables are resolved from the
   static `BUILTIN_TABLES` by `builtin_table(namespace, name)` at ingress; no
   Postgres migration seeds catalog rows for `eval.runs`/`eval.assertions`
   (`git grep` of both migration dirs for `'eval'`/`'runs'`/`'assertions'` is
   empty). No persisted catalog state is orphaned by the code removal, and
   REQ-116 waives retained data regardless.
7. **Stored Card JSON readability.** At base, `publishes_to` on Agent, Service,
   and Service components was `skip_serializing_if = "Vec::is_empty"`, so only
   Cards that actually published to (unshipped) Drift/Eval Cards persisted the
   field. Stored Cards of shipped kinds without publication targets remain
   readable. Covered by REQ-109's no-migration statement.
8. **Registration atomicity.** `validate_effective_bindings`
   (`resolve.rs:66`) runs inside `resolve_external` (`service.rs:891`) before
   `write_registration` opens the audited write transaction, so every binding
   rejection (unresolved, wrong kind, Workflow `on_failure`, Trigger/implementation
   mismatch) persists nothing. Referenced bodies are read by exact identity and
   same-version Cards are immutable (`WYRD_REGISTRY_409_SPEC_DRIFT`), so the
   pre-transaction read cannot observe a spec that differs at write time; this
   follows the pre-existing ref-resolution pattern. Integration evidence:
   `unresolved_dependency_leaves_no_registration_operation ... ok` in
   `cards_integration.log`.

## Verification Limits

Did not run cargo/mise/psql. Orchestrator results at review time
(`scratchpad/verify/summary.txt`): `diffcheck`, `fmt`, focused t1–t4, `wyrdspec_lib`,
`codegen`, `client_tier`, `pyo3_scope`, `test_shared`, `cards_unit`,
`cards_integration` (23 passed, real Postgres, so `Verifier` rows satisfy the new
constraint), `cli_journey` all rc=0. `wyrdstate_journey` was in progress;
**`test_sql`, `lints`, and `test_wyrd` had not yet run**, so migration
application from an empty database in the `test:sql` lane and the vala-sql
file-list/migration-count tests are not yet evidenced. The verdict below is
conditional on `test_sql` passing. No SQL-level test asserts that the DB now
rejects `kind='Drift'`/`'Eval'`; the server rejects them first (REQ-109), and the
CHECK is defense-in-depth, so this is not a coverage gap tied to an obligation.

## Findings

None material.

Non-blocking observation (not a finding; untouched code): the rustdoc on
`EvalWorkflowSummary` and `EvalReport::workflow_summary`
(`crates/vala/vala-eval/src/executor.rs:86,119`) still says it populates
`vala.eval.runs`, a table REQ-116 removed. It creates no persistence path, but a
reader could infer the table still exists; worth a one-line doc fix when that
file is next touched.

## Verdict

**PASS** (conditional on the pending `test_sql` lane passing).
