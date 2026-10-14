# TASK-001 — Make sub/act the only work-attribution contract

Task ID: `TASK-001`  
Approved spec: `../spec.md`, revision 1  
Mapped obligations: REQ-001..REQ-007, INV-001..INV-005, AC-001..AC-009  
Dependencies: none beyond the approved specification and current owners  
Implementation skill: `$wyrd-implement`

## Outcome and Value

Every in-scope work row identifies the principal whose authority and budget the
work used (`sub`) and the principal or tenant SYSTEM process that performed it
(`act`). Both values are non-null, server-derived, tenant-local, and projected
consistently through Postgres, Bifrost/Iceberg, Scribe, gateway, verification,
audit, registration, public wire types, and the Rust, Python, and TypeScript
SDKs. Card columns describe their roles without aliases. Fresh schema history
and dev fixtures establish the contract without migrations, backfills, or
compatibility paths.

This is one outcome-complete task by owner direction. Splitting the storage,
runtime, or SDK cuts would temporarily assign different meanings to the same
public rows and Bifrost fingerprints.

## Owners, Scope, Consumers, and Prohibited Changes

The implementation extends the current owners rather than introducing another
attribution service:

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Verified caller attribution | `Caller` and `RequestContext` in `crates/wyrd/wyrd-server/src/components/auth/caller_extractor.rs` and adjacent request context | HTTP/MCP handlers, gateway invocation, direct verification, audit staging; delegated-token tests | Callers expose an effective principal and chain, but work rows do not have one canonical subject/actor projection | Derive the pair once from the verified caller boundary: direct calls use caller/caller; delegated calls use original subject/outermost actor | None; the verified caller already owns trustworthy identity and delegation data |
| Bifrost managed columns and stamping | `MANAGED_COLUMNS` in `crates/vala/vala-bifrost-redux/src/tables/managed_columns.rs`; Gate/Scribe frame validation and stamping | Observation table schemas, Scribe ingest, claims/sampling, reserved-column tests, server/SDK Bifrost journeys | Managed `principal_id` represents the writer and cannot express authority versus actor | Replace it in place with ordered `sub_principal_id`, `act_principal_id`; stamp both only from trusted server attribution | None; managed columns and existing stamping are the authoritative path |
| Gateway authority, capture, and spend | `GatewayInvocation::{admit_call,attribute,route,govern}` in `components/gateway/invocation.rs`, plus `capture.rs` and `ledger.rs` | public and judge gateway callers; Rust/Python/TS gateway journeys; Gate tests | capture duplicates `caller_principal_id`; internal judge authority and accounting can follow the wrong principal | Carry the resolved pair through the existing invocation, authorize/account against `sub`, preserve delegated subject/actor intersection, and capture only the managed pair | None; the gateway invocation already coordinates authorization, admission, accounting, routing, and capture |
| Verification run authority and results | `VerifierRunQueue`/run rows in `wyrd-sql/src/queries/verifier_runs.rs`; server verification runner, direct/eval/results paths; `ScribeOutbox::VerifierAttribution` | scheduled, observation, and direct verification tests and journeys | internal runs/results use SYSTEM or observation writer as requester; judge calls do not consistently execute as binding owner | Resolve the binding owner's Service/Agent principal as `sub`, use tenant SYSTEM as `act` for Wyrd-started work, preserve caller/caller for direct work, and use that authority for judge calls and results | None; run queue, runner, and result outbox already own the lifecycle |
| Audit attribution | Scribe audit outbox and `vala.system.audit_log` schema/projection | permission decisions from gateway, registration, verification, and audit queries in journeys | audit has one `audit_principal_id` and kind | Project `sub` id/kind plus `act` id while keeping the delegation chain in detail; audit remains non-blocking | None; the existing audit outbox remains the single write path |
| Registration attribution and retry identity | Cards, service-account/API-key, operator-connection, and Bifrost-table registration writers; `wyrd.card_registration_operations` | registration handlers/services and SQL/integration tests | creator/actor columns are single-valued; idempotency is not consistently actor-keyed | Write the pair from verified caller context and key retry identity on `act` exactly where specified | None; each existing registration owner remains responsible for its row |
| Durable schemas | defining SQL migrations under `crates/wyrd/wyrd-sql/migrations`, `crates/vala/vala-sql/migrations`, and built-in Bifrost table declarations | SQL migration tests, built-in schema/fingerprint tests, query row mappings | defining history and Iceberg schemas contain superseded names and nullable/single attribution | Edit the defining declarations in place, fold obsolete ALTER-only history, and delete `20261014000001_verifier_run_requester_every_origin.sql` | None; the approved decision expressly forbids an evolution layer |
| Public contracts and language clients | `wyrd-spec` verification/gateway/Vala/audit types and managed-column constants; `wyrd-client`; Rust/Python/TS SDK projections and generated outputs | typed reads, stubs/declarations, codegen, unit and integration journeys | public rows and reserved-column checks expose old names or only one principal | Rename the actual contract, reserve both new managed names, regenerate owned artifacts, and update all three language surfaces without aliases | None; existing generators and thin SDK projections own this closure |
| Tenant SYSTEM fixture parity | production SYSTEM provisioning and `wyrd_dev_fixtures::pg::seed_tenant` | provisioning tests and `pg::pg_tests::fixture_smoke` | fixture tenants can lack active SYSTEM | Make seed behavior establish the same active tenant SYSTEM invariant | None; the fixture seed already owns test-tenant creation |

Scope includes every named table and its readers/writers, verification runtime
authority, gateway authorization and spend, audit, Scribe stamping, generated
contracts, SDK projections, SYSTEM fixture seeding, and the two architecture
authorities. Existing origin references and trace/span links keep their current
meaning.

Prohibited changes:

- Do not add a migration, backfill, compatibility view, alias, fallback, or
  fingerprint-evolution path. Delete
  `crates/wyrd/wyrd-sql/migrations/20261014000001_verifier_run_requester_every_origin.sql`.
- Do not accept either principal from client columns, headers, trace context,
  or observation values; both are derived from verified identity or the
  tenant-local binding owner and SYSTEM principal.
- Do not make either value nullable, use SYSTEM as `sub`, or treat the
  observation writer as the observation-triggered run subject.
- Do not change token issuance, delegated exchange, permissions, roles, trace
  propagation, the identity/owner tables exempted by REQ-005, or
  `auth_connection_test_state.tester_principal_id`.
- Do not move durable behavior into an SDK, add an attribution framework, or
  duplicate the existing Gate, Scribe, verification, audit, or registration
  owners.

## Approach

1. Replace the persistent contract at its definitions: update the ordered
   Bifrost managed-column set and system schemas, edit the original Postgres
   `CREATE TABLE` migrations, fold now-obsolete ALTER-only history, and remove
   `20261014000001` without adding schema evolution.
2. Extend the verified-caller, binding-owner, and tenant-SYSTEM paths to supply
   one non-null pair; carry it through Scribe and audit staging while preserving
   tenant checks and server-only stamping.
3. Make gateway authorization/admission/spend and verification judge execution
   use `sub`; preserve the external delegated intersection; make direct,
   scheduled, and observation-triggered rows use the approved actor semantics.
4. Update registration, run, result, audit, Bifrost-table, and Card-role readers
   and writers, including actor-keyed idempotency and verified-Card versus
   Verifier-Card meanings.
5. Update `wyrd-spec`, shared client code, all three first-class SDKs, generated
   schemas/stubs/declarations, and architecture documentation with no aliases.
6. Prove the behavior in execution order with focused storage/runtime tests and
   complete Rust/Python/TypeScript journeys, then run only the required scoped
   owner and boundary lanes.

## Ordered Implementation Scenarios

### Scenario 1 — Persistent schemas expose one complete pair

**Behavior.** Fresh Postgres and Bifrost definitions use the exact names and
roles in REQ-004 and REQ-005. Every Observation-policy table appends, in order,
`run_id`, `card_uid`, `sub_principal_id`, `act_principal_id`,
`wyrd_request_id`, `wyrd_ingested_at`. Both principals are non-null where work
is stored. Results use managed `card_uid` for the verified Card and explicit
`verifier_card_uid`. `20261014000001` is absent and an empty database applies
cleanly (AC-006, AC-007).

**RED.** First change the schema/fingerprint and migration assertions so they
expect the approved pair and role-specific Card names; they fail on the current
managed `principal_id`, old role names, nullable requester history, and the
still-present `20261014000001`.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::managed_columns::tests::observation_policy_appends_every_managed_column_in_order)'
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sql --test integration -E 'test(=pg_migration::pg_tests::migrations_apply_and_are_idempotent)'"
```

**GREEN.** Edit each defining `CREATE TABLE` and built-in Iceberg schema in
place, update row mappings and schema fingerprints together, fold/delete only
ALTER history made obsolete by those definitions, and delete the owner-named
migration. Do not add migration or compatibility logic.

**REFACTOR.** Remove obsolete single-principal and old Card-role schema
constants only after every schema consumer compiles and both focused tests stay
green.

### Scenario 2 — Scribe rejects client attribution and stamps the trusted pair

**Behavior.** A frame cannot declare either managed attribution column. Gate
and Scribe append the server-resolved pair to all Observation-policy rows,
including gateway, verification, eval, drift, and audit-backed work, without
trusting row content (REQ-001, REQ-006, INV-001, AC-005).

**RED.** Extend the existing managed-column/reserved-column tests and each
SDK's Bifrost write journey to submit `sub_principal_id` and
`act_principal_id`; each currently escapes the old reserved set or lacks the
new stamped values.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::managed_columns::tests::observation_policy_appends_every_managed_column_in_order)'
cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q tests/unit/bifrost/test_reserved_column.py::test_a_server_owned_column_is_refused_before_any_request
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=query_bifrost::reserved_attribution_columns_are_refused)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/bifrost/test_table_time_column.py::test_reserved_attribution_columns_are_refused"
cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/bifrost-write.test.ts -t 'reserved attribution columns are refused'
```

**GREEN.** Replace the managed name and payload plumbing through the existing
validation, Gate admission, Scribe stamping, peer/outbox attribution, and table
schemas. Keep the existing reserved-column error and make both columns
server-owned in Rust, Python, and TypeScript.

**REFACTOR.** Consolidate duplicate old-name lists into the existing managed
column source only where current generation/projection patterns already permit
it; add no new registry or validation layer.

### Scenario 3 — Direct and delegated gateway work records the correct authority

**Behavior.** Direct gateway calls record `sub = act = caller`; a delegated
call by B on behalf of A records `sub = A`, `act = B`. Authorization retains
the subject/actor intersection for delegated external calls, admission and
spend are charged to `sub`, and audit rows match the gateway pair while keeping
the delegation chain in detail (REQ-002, REQ-003, INV-003, AC-003, AC-004).

**RED.** Extend the existing Rust gateway journey with direct, delegated,
permission, spend, Bifrost, and audit assertions; extend the Python and
TypeScript direct cases. They fail while gateway capture/audit use a single or
duplicated caller field and accounting follows the effective writer.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=gateway_inference::gateway_call_records_whichever_attribution_it_carries)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=gateway_inference::delegated_gateway_call_records_subject_and_actor)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_gateway_inference.py::test_gateway_call_records_a_lone_run_or_card"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/gateway-inference.test.ts -t 'gateway call records a lone run or card'"
```

**GREEN.** Project the pair from the existing verified caller into
`GatewayInvocation`, keep the current delegated permission intersection,
authorize/account on `sub`, and send the same pair to capture and the
non-blocking audit outbox. Remove the payload-level caller duplicate.

**REFACTOR.** Keep one gateway attribution value flowing through the existing
invocation phases; remove conversion code made redundant by the contract
rename while preserving current timeout, authorization, and audit-failure
semantics.

### Scenario 4 — Verification runs, judge calls, and results use execution authority

**Behavior.** Scheduled and observation-triggered runs resolve each binding's
owner Service/Agent principal as `sub` and tenant SYSTEM as `act`; the
observation writer remains reachable only through `source_record_id`. Judge
calls authorize and spend as the run's `sub`, with SYSTEM imposing no extra
restriction. Direct verification uses caller/caller. Runs, judge gateway rows,
result summaries/items/features, and audit rows agree on the pair and on
verified `card_uid` plus `verifier_card_uid` (REQ-002..REQ-004, INV-002..INV-005,
AC-001..AC-003).

**RED.** Replace the superseded writer-owned SQL and server-journey assertions,
then extend scheduled, observation, and direct journeys. The observation case
must use a writer different from the binding owner, prove a judge refusal when
the owner lacks `gateway:invoke`, and prove spend lands on the owner.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sql --test integration -E 'test(=pg_verifier_runs::observation_runs_use_binding_owner_attribution)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=eval_verification::eval_runs_use_each_binding_owner_not_the_observation_writer)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=eval_verification::scheduled_eval_uses_owner_subject_and_system_actor)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verify_in_real_time::judge_call_is_attributed_to_the_run_and_card)'"
```

**GREEN.** Resolve the tenant-local binding owner and active SYSTEM principal
through existing SQL/runtime owners, store the pair on the run, execute judge
calls with subject authority, and carry the same pair plus Card roles through
the existing result outbox. Direct verification derives both roles from the
verified caller. Preserve source, trigger, span-link, run, and binding origins.

**REFACTOR.** Delete superseded requester/writer attribution branches and old
Card-role conversions after scheduled, observation, and direct tests all pass;
do not alter trace propagation or invent a verification-specific auth layer.

### Scenario 5 — Registration and tenant fixtures satisfy the same invariant

**Behavior.** Card, service-account, API-key, operator-connection, and
Bifrost-table registrations persist server-derived caller/caller creator pairs;
card-registration retries key on `act`. Every fixture-seeded tenant has one
active SYSTEM principal matching production provisioning (REQ-001, REQ-005,
REQ-007, AC-008).

**RED.** Update the nearest registration SQL/integration assertions for both
creator roles and actor-keyed idempotency, and extend `fixture_smoke` to require
an active tenant SYSTEM principal. These fail on the current single columns and
fixture seed.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-dev-fixtures --features pg --lib -E 'test(=pg::pg_tests::fixture_smoke)'"
```

**GREEN.** Update the existing registration writers/readers and original table
definitions to store the pair, change only the specified retry unique key to
`act`, and reuse production SYSTEM provisioning semantics in `seed_tenant`.

**REFACTOR.** Share existing principal-construction SQL or domain values where
the owning packages already expose them; do not introduce fixture-only identity
semantics or a cross-owner registration abstraction.

### Scenario 6 — Public contracts and all first-class SDK journeys agree

**Behavior.** Wire types, generated schemas, Rust/Python/TypeScript typed rows,
read paths, stubs, and declarations expose only the approved pair and Card-role
names. Direct gateway/direct verify and reserved-column journeys pass in all
three SDKs. No old attribution name remains in schema, wire, SDK, generated, or
the two architecture authorities (REQ-006, AC-003, AC-005, AC-006).

**RED.** Change public assertions and typed journey reads before production
projections. Existing compilation/codegen and the language journeys fail on
old fields, missing `act`, and old reserved-column constants.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verify_in_real_time::judge_call_is_attributed_to_the_run_and_card)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_verify_in_real_time.py::test_judge_call_is_attributed_to_the_run_and_card"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/verify-in-real-time.test.ts -t 'judge call is attributed to the run and card'"
```

**GREEN.** Update the owning `wyrd-spec` types and constants, shared Rust
client, thin language projections, package exports, generators, and generated
outputs. Regenerate rather than hand-edit generated stubs/declarations. Update
`architecture/wyrd-design.md` and `architecture/bifrost-design.md` to the new
rule. Remove aliases and stale old-name prose in the approved surfaces.

**REFACTOR.** Remove obsolete conversion/accessor aliases only after the three
runtime journeys and codegen agree; retain current SDK layering and do not add
language-specific durable logic.

## Acceptance Criteria

- Every direct, delegated, scheduled, and observation-triggered flow produces
  the exact non-null pair required by REQ-002, and every stored pair is derived
  from verified identity or tenant-local binding owner/SYSTEM resolution.
- Gateway and judge authorization plus spend use `sub`; external delegation
  still uses the subject/actor permission intersection; SYSTEM neither grants
  nor restricts authority.
- Bifrost's six managed columns have the approved names and order. Client
  frames declaring either principal column fail with the existing reserved
  column error in Rust, Python, and TypeScript.
- Gateway, verification, eval, drift, and audit system tables use the approved
  attribution and Card-role columns. Managed verification `card_uid` is the
  verified Card and `verifier_card_uid` identifies its Verifier.
- All named PostgreSQL tables, their unique constraints, writers, and readers
  match REQ-005. Plain `principal_id` remains only in the explicit identity or
  owner exemptions, and `tester_principal_id` is unchanged.
- `20261014000001_verifier_run_requester_every_origin.sql` is deleted, no new
  migration/backfill/compatibility path exists, and an empty Postgres instance
  applies the edited history.
- Every fixture-created tenant has an active SYSTEM principal; SYSTEM is never
  `sub` and is `act` only for Wyrd-started work.
- Public Rust wire types and Rust/Python/TypeScript SDK projections, generated
  schemas/stubs/declarations, and both architecture documents contain no
  superseded attribution/Card-role names and expose no aliases.
- Existing origin references, tenant isolation, audit non-blocking behavior,
  and span-link/trace semantics remain unchanged.
- AC-001 through AC-009 have passing focused evidence and scoped lane output.

## Expected Write Set and Consumer Closure

Paths are ownership guidance, not a private implementation allowlist. The
implementer must follow actual compiler/query consumers reached from these
owners.

- Architecture: `architecture/wyrd-design.md`,
  `architecture/bifrost-design.md`.
- Postgres definitions and SQL owners:
  `crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql`,
  `20260601000006_cards.sql`, `20260601000011_auth_relax_created_by_fk.sql`
  if rendered obsolete by the defining edit,
  `20260601000014_card_registration_operations.sql`,
  `20260601000027_verification_bindings.sql`,
  `20260601000028_system_principal.sql`,
  `20260601000029_verifier_runs.sql`,
  `20260601000031_verifier_run_idempotency.sql`,
  `20260601000032_drift_baselines.sql`,
  `20261001000000_operator_connections.sql`, and deletion of
  `20261014000001_verifier_run_requester_every_origin.sql`; corresponding
  `wyrd-sql/src/queries` and server row consumers.
- Vala SQL definition:
  `crates/vala/vala-sql/migrations/20260619000001_olap_minimal.sql` and its
  Bifrost-table registration/query consumers.
- Bifrost schemas and transport:
  `crates/vala/vala-bifrost-redux/src/tables/{managed_columns,gateway,verification,eval,drift,audit}`
  plus Gate validation/stamping, Scribe ingest/outbox/peer attribution, claims
  and sampling consumers that still interpret the managed principal.
- Server identity and runtime:
  `crates/wyrd/wyrd-server/src/components/auth`, gateway
  `invocation.rs`/`capture.rs`/`ledger.rs`, verification runner/direct/eval/
  results/service paths, Scribe outbox, audit staging, platform provisioning,
  and Card/service-account/API-key/operator-connection/Bifrost-table
  registration owners.
- Contracts: `crates/wyrd-spec/src/verification.rs`, gateway records, Vala API,
  audit detail, managed-column constants, schemas/proto and generated contract
  outputs reached from them.
- Clients: `crates/shared/wyrd-client` Bifrost, gateway, cards, principals, and
  operator-connection projections as required; `sdks/wyrd-sdk-rust`,
  `sdks/wyrd-sdk-python` wrappers/exports/generated stubs, and
  `sdks/wyrd-sdk-ts` bindings/types/declarations.
- Fixtures and tests: `crates/shared/wyrd-dev-fixtures/src/pg.rs`; SQL migration,
  verifier-run, registration and provisioning tests; Bifrost schema/Gate/Scribe
  tests; gateway and server journeys; Rust/Python/TypeScript SDK unit and
  integration journeys.

No new crate, dependency, service, store, transport, generator, compatibility
module, or abstraction is expected.

## Verification and Evidence

Run every exact focused command in the scenario that names its test. Confirm
planned replacement/new selectors with `mise exec -- cargo nextest list` after
the tests exist; a zero-test selection is a failure. Then run the scoped owner
and boundary lanes required by the specification:

```bash
mise run fmt
mise run lints
mise run test:wyrd
mise run test:vala
mise run test:shared
mise run test:sql
mise run test:bifrost
mise run test:gateway:gate
mise run py:format
mise run py:lints
mise run py:typecheck
mise run py:test:unit
mise run py:test:integration
mise run ts:format:check
mise run ts:lints
mise run ts:typecheck
mise run ts:test:unit
mise run ts:test:integration
mise run codegen:check
mise run check:deps
mise run check:tenant-isolation
git diff --check
```

Static closure for AC-006 is a repository search scoped to schema declarations,
wire/public types, SDK sources, generated artifacts, and
`architecture/{wyrd-design,bifrost-design}.md` for
`caller_principal_id|requested_by_principal_id|subject_card_uid|audit_principal_id|verifier_uid|created_by`
and managed attribution `principal_id`. Hits in the REQ-005 identity/owner
exemptions, `tester_principal_id`, immutable change records, or unrelated local
implementation vocabulary are not contract failures. Review the migration diff
to prove no new migration, backfill, compatibility view, or fingerprint
evolution path was added and that `20261014000001` was deleted.

Do not run `mise run gate` for this task. The approved spec names the scoped
lanes, and the one task already supplies the integrated user journeys.

## Material Stop Conditions

Stop implementation and return for specification/planning authority if:

- a required flow cannot derive a single tenant-local binding-owner principal
  or active SYSTEM principal without changing Card ownership semantics;
- preserving the delegated permission intersection conflicts with charging
  admission/spend to `sub`;
- a shipped-data or deployed-schema constraint is discovered that would require
  migration, backfill, compatibility, nullable attribution, or fingerprint
  evolution;
- a public consumer requires an old field alias or mixed old/new contract;
- completing the task requires changing token issuance/delegated exchange,
  adding a permission or role, changing trace propagation, or altering an
  exempt identity/owner table;
- the current Gate/Scribe, gateway, verification, audit, registration, or SDK
  owner cannot retain the behavior without a new durable owner or dependency.

Ordinary choices of private helper, local data flow, fixture construction, and
test arrangement remain `$wyrd-implement` decisions and are not stop
conditions.

## Authority Links

- `../spec.md` revision 1, approved by the human owner
- `../../../../AGENTS.md`
- `../../../../architecture/agent-rules.md`
- `../../../../architecture/wyrd-design.md`
- `../../../../architecture/wyrd-doctrine.mdx`
- `../../../../architecture/bifrost-design.md`
- `../../../../architecture/references/languages/spec-driven-development.md`
- `../../../../architecture/references/languages/implementation-execution.md`
- `../../../../architecture/references/languages/testing-workflows.md`
