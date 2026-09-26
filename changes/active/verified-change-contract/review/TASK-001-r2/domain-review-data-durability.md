# TASK-001 r2 — Data, Durability, and Concurrency Domain Review

Reviewer: `domain-rev` (persistent data, migrations, durability, and
concurrency).

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/verified-change-contract`
- Base: `5293546f33b3a5fd9de529098e23ea70d472c412`
- Cumulative candidate: `dd0503e7149017d760a987346e2838001e5c3429`
- Working-tree HEAD: `5f14f3c325cc8c45081fe9ad0543e7794b4a4e0f`
  (the repository-rule clarification after the candidate is authority, not
  reviewed task work)

## Reviewed boundary

- The new `wyrd.cards` kind constraint migration and retirement migration for
  `vala.drift_alerts`.
- Removal of `vala.eval.runs` and `vala.eval.assertions` from the closed Bifrost
  built-in registry and deletion of their table owners.
- Deletion of the `vala.drift_alerts` SQL queries, row types, and integration
  target.
- Card-loader and registry seams that resolve, UID-pin, validate, and persist
  `verified_by` references, including rejection ordering and no-write proof.
- The round-1 remediation changes to Bifrost verification lanes and the
  Postgres-backed ingest authentication test.
- Concurrent restart/runtime proof exercised by `mise run test:bifrost`, with
  specific review of the recorded pool-identity and Oracle-settlement flakes.

## Authority and source coverage

| Boundary | Authority | Source inspected | Result |
|---|---|---|---|
| Card-kind persistence | Spec REQ-109; TASK-001 Scenario 1; `AGENTS.md` §§2, 9, 11–12; `architecture/v1/00-foundations/{postgres-layout,sql-foundation}.md` | `crates/wyrd/wyrd-sql/migrations/20260601000025_verifier_card_kind.sql` and prior `cards_kind_check` owner | PASS |
| Retired durable tables | Spec REQ-116; TASK-001 Scenario 3 and AC-022; `architecture/bifrost-design.md`; analytical reliability authority | `crates/vala/vala-sql/migrations/20260910000028_drop_drift_alerts.sql`, deleted Vala SQL owners, `vala-bifrost-redux/src/tables/mod.rs` | PASS |
| Atomic registration | Spec REQ-090–094, REQ-143; TASK-001 Scenario 2; SQL transaction discipline | `wyrd-server/src/components/cards/{service,resolve}.rs`, `wyrd-spec/src/graph/composition.rs`, loader resolution/validation, `pg_card_registration_route.rs` | PASS |
| Verification and concurrency | TASK-001 remediation §7/AC-R1; `AGENTS.md` §§11–12, especially “Pre-existing is not a waiver”; `architecture/references/domain/analytical-operations-reliability.md` | `mise.toml`, `scripts/run-bifrost-tests.sh`, `wyrd-testing/src/{server,bifrost/cluster}.rs`, `wyrd-testing/tests/bifrost/oracle/distributed.rs`, committed remediation evidence | FAIL |
| Tenant-admission test repair | `AGENTS.md` test and completion rules; tenant/RLS authorities | `pg_grpc_ingest_smoke.rs`, `wyrd-auth/src/revocation_resolver.rs`, repository JWT-minting test call sites | PASS |

### Data and durability conclusions

- Both SQL migrations are new highest-version files in their owning migrators;
  no applied migration was edited. The `cards_kind_check` replacement is the
  old set minus `Drift`/`Eval` plus `Verifier` and retains the non-registrable
  `External` discriminator. The specification explicitly waives historical-row
  conversion because the retired kinds did not ship.
- `DROP TABLE vala.drift_alerts` is valid after the unconditional earlier table
  creation; its policy, grants, and indexes are table-owned, and no later
  migration or production query owner still depends on it. REQ-116 explicitly
  authorizes deletion without retained-data migration.
- `eval.runs` and `eval.assertions` were code-defined built-ins, not
  Postgres-seeded catalog rows. They no longer resolve through `BUILTIN_TABLES`,
  and the remediation added them to the existing removed-name assertion.
- Pure binding validation runs in `validate_request`; external reference and
  effective-spec validation complete on a tenant-scoped read connection before
  `write_registration` opens the audited write transaction. The real-server
  refusal cases assert no registration writes. The remediation preserves that
  ordering while centralizing binding-site enumeration.
- The ingest test repair seeds the only arbitrary tenant passed to the local
  `mint_user_jwt` helper. Other call sites in that file seed their arbitrary
  tenants, and the other server tests mint against the fixture's already-seeded
  tenant. No second reachable unseeded-tenant test was found in the reviewed
  test surfaces.

## Material proposed findings

### DDR2-1 — Pool freshness is asserted with allocator addresses

- **Classification:** `VIOLATION`
- **Violated obligation:** TASK-001 remediation requires `mise run
  test:bifrost` as closure proof; `AGENTS.md` §12 requires every relevant gate
  to pass and explicitly forbids accepting or recording a pre-existing flaky
  assertion instead of fixing it.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/bifrost/cluster.rs:2766-2778` and
  `:2833-2859`; identity is produced by
  `crates/wyrd/wyrd-testing/src/server.rs:2923-2929`.
- **Evidence:** `postgres_pool_identity` casts the addresses of two `PgPool`
  wrapper values to `usize`. `stop_node` takes and drops the old server before
  `restart_node` constructs the replacement, so the allocator is free to reuse
  both addresses. The two `assert_ne!` checks therefore fail for a valid fresh
  pool graph. The candidate's committed remediation evidence records exactly
  this failure and accepts a retry; the test is selected by
  `test:bifrost:unit:rust:inner`, hence by the required aggregate gate.
- **Observable consequence:** an unchanged correct restart can fail the required
  Bifrost gate depending on allocator reuse, while a passing address comparison
  does not prove that the old pools closed or the replacement pools work. The
  candidate therefore lacks credible deterministic closure evidence.
- **Required testable correction:** delete the address-identity freshness
  checks and the test-only pointer-identity surface if it has no remaining
  caller. Prove the lifecycle behavior through the existing owners: the stopped
  node is absent, the restarted node's application and Vala pools execute their
  existing health/query operations, and the surviving nodes remain usable.
  Keep the retained-root and re-derived-resource-plan assertions unchanged.
  The focused cluster tests and `mise run test:bifrost` must then pass without
  retrying a failed assertion.

### DDR2-2 — Oracle cleanup is sampled once despite asynchronous terminal cleanup

- **Classification:** `VIOLATION`
- **Violated obligation:** the same remediation verification requirement and
  `AGENTS.md` §12 pre-existing-failure rule; the analytical reliability
  authority requires cancellation to release descendant reservations and
  temporary ownership.
- **Exact location:**
  `crates/wyrd/wyrd-testing/tests/bifrost/oracle/distributed.rs:318-327`.
- **Evidence:** after the tripwire query returns its terminal outcome, the test
  immediately takes one `oracle_inspection` sample and fails if asynchronous
  graph cleanup has not yet released every active, queued, memory, spill, or
  peer counter. It has no completion wait or bounded convergence boundary. The
  candidate's committed remediation evidence records an “Oracle runtime did not
  settle” failure and accepts the rerun as green.
- **Observable consequence:** the required journey gate can fail solely because
  it races terminal cleanup, so the recorded 9/9 result does not establish a
  repeatable clean runtime boundary.
- **Required testable correction:** make this existing assertion observe the
  real cleanup boundary: wait, under one bounded deadline, until the existing
  `oracle_inspection` ownership fields are all zero, then retain the exact zero
  assertions and fail with the last inspection if the deadline expires. Do not
  weaken or delete the resource-release invariant. Run the exact Oracle journey
  and `mise run test:bifrost` without accepting a retry.

## Verification limits

- I did not rerun the multi-hour aggregate gates. I inspected the complete
  base-to-candidate source and the verification results committed in the
  immutable candidate. Those results report all requested lanes eventually at
  exit 0, but also report the two failed assertions above before rerun.
- I did not inspect or opine on unrelated Drift/Eval scoring semantics, public
  SDK shape, or security/RBAC decisions except where needed to trace the
  persistence and tenant-admission seams.

## Overall result

**FAIL**

The migration, table-retirement, registration-atomicity, and tenant-seeding
work passes this domain review. `DDR2-1` and `DDR2-2` remain material because
the candidate knowingly closes a required gate by retrying two nondeterministic
assertions, which current repository authority expressly forbids.
