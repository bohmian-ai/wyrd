---
id: TASK-001-R1
kind: remediation
status: ready
spec: SPEC-sub-act-attribution
spec_revision: 5
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-010, REQ-011, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, AC-001, AC-006, AC-007, AC-008, AC-009, AC-011, AC-013, AC-015, AC-016, AC-017, AC-018, AC-019]
depends_on: []
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-4, FIND-TASK-001-5, FIND-TASK-001-6, FIND-TASK-001-7, FIND-TASK-001-8, FIND-TASK-001-9]
---

# Close subject and actor attribution gaps

Execute directly through `$wyrd-implement`. This is one remediation task for
TASK-001, using approved specification revision 5 and the independently
validated review. It packages the diagnosed corrections; it does not reopen
architecture or prescribe private implementation mechanics.

## Authority and immutable inputs

- Approved spec: [spec.md](../../spec.md), **revision 5**.
- Original task: [TASK-001-sub-act-attribution.md](../../tasks/TASK-001-sub-act-attribution.md).
- Original cumulative base: `b22718f2cf5da0ebcb7f26bc376157e5eec7768f`.
- Reviewed implementation candidate: `518a5d8b822a4e649a5fe90b14f3eb2b1fc8a0aa`.
- Approved r5 and preserved review: `69687f6480592edc2866d241970c62996702fe4a`.
  This is the starting revision for this task. Its difference from the reviewed
  candidate is the review packet and spec settlement, not an implementation fix.
- Historical verdict: [verdict.md](verdict.md), `SPEC_REVISION_REQUIRED` under r4.
- Validated diagnoses, source traces and rejected claims:
  [findings-validation.md](findings-validation.md).
- Repository authority: `AGENTS.md`, `architecture/agent-rules.md`,
  `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`,
  `architecture/bifrost-design.md`, `architecture/wyrd-security-posture.md`,
  and the applicable references routed through `architecture/references/README.md`.

Preserve the historical review and original task. Revision 5 supersedes their
r4 authentication-refusal instruction; do not silently rewrite that history or
claim implementation closure from a changed spec. Subsequent review includes
the original task, r5, this task, prior verdict/ledger, and the full cumulative
base-to-new-candidate diff, using the remediation diff to locate changed owners.

### Settlement and finding disposition

**FIND-TASK-001-1 is now bounded by approved r5.** Authenticated platform
provisioning records the verified platform operator pair on newly created
SYSTEM and built-in rows. Creation genuinely lacking a verified caller uses
the existing `PLATFORM_AUDIT_PRINCIPAL` pair: SYSTEM bootstrap, startup
reconciliation, reserved audit-owner tables, defining SQL backfill and fixture
seeding. These are explicit lookup exceptions; the sentinel grants no authority.
SYSTEM never creates itself. Existing pairs remain immutable on retry/reconcile.

**FIND-TASK-001-3 is superseded by r5 REQ-010/AC-015.** Missing, malformed and
unverifiable bearer credentials keep their existing rejection responses and
produce no audit decision before identity resolves. Existing audited credential
exchange refusals continue, using the sentinel when unresolved. Do not implement
the r4 recommendation to add general authentication-refusal audit emission.
This finding is deliberately excluded from `remediates`; preserve the stable
ID and record its approved disposition in implementation/review evidence.

Findings 2 and 4–9 retain their validated correction boundaries. No new finding
IDs or product decisions are introduced by this task.

## Intended outcome and owners

Every remaining attribution loss is corrected at its existing producer:
provisioning/catalog, shared-client Workflow preparation, observation acceptance,
credential audit, and Verifier read authority. Required proof reaches the real
protected consumer. Public documentation agrees with r5 and actual direct versus
queued behavior. Existing authentication refusal behavior stays intact.

Owners are `wyrd-sql`/`vala-sql` for durable values, server platform provisioning
and `BuiltinTables` for lifecycle, Bifrost catalog for registration, shared
`wyrd-client` for local Workflow execution, server verification for accepted
execution/read context, `wyrd-auth` for credential audit, and Oracle for its
existing query context/audit projection. Extend these owners; no new owner,
resolver, queue, transport, permission evaluator or SDK-specific workflow is needed.

## Diagnoses and selected corrections

### 1. Creator provenance — FIND-TASK-001-1

**Violation and evidence.** REQ-002/005/007/010, INV-001/002/007 and AC-019.
`20260601000028_system_principal.sql:74–79` inserts the new SYSTEM ID as both
creator fields. Its wrapper in `queries/auth/service_accounts.rs:254` accepts
no creator context. `TenantProvisioning::establish_tenant_administration`
(`components/platform/provisioning.rs:447–456`) already receives real operator
attribution and uses it for the admin/key, but drops it for SYSTEM. The same
operation drops it before `BuiltinTables::ensure_tenant` at lines 287–290;
`BifrostCatalog::ensure_builtin` (`catalog/bifrost_catalog.rs:926–932`) always
supplies the sentinel. Creator rows therefore misstate available provenance.
Startup, migration backfill and fixture seeding have no verified creator.

**Recommendation.** Carry the already verified creation pair through the existing
SYSTEM provisioning and built-in registration owners. Use the approved sentinel
pair explicitly at their genuinely callerless entry points. Keep the same SQL
provisioning function, catalog registration/upsert and canonical inventory; do
not introduce an alternate insert or infer a platform root, tenant admin or
identity from a database login. Preserve transaction ownership, tenant isolation,
eager ensure/reconciliation, one active tokenless SYSTEM and immutable first
creation values. Existing catalog rows that already carry the sentinel remain
unchanged when reconciliation or a later authenticated ensure revisits them.

Reach all callers: authenticated provisioning/recovery, startup reconciliation
including reserved audit-owner tables, defining backfill, `wyrd-dev-fixtures`
seeding, and server/test fixtures. Edit defining SQL in place; no migration or
backfill for deployed data is authorized. Update the owning architecture's
creator/sentinel descriptions to r5 in the same change.

**Proof.** P1 reads persisted creator pairs before and after real startup/retry:
new authenticated tenant SYSTEM/built-ins use the operator pair; autonomous
creation uses sentinel/sentinel; no SYSTEM subject; re-ensure/restart preserves
the original pair. P2 proves the defining SQL and fixture SYSTEM path use the
same rule from an empty database. Exercise the reserved audit owner as well as
ordinary tenants; platform IDs must not be rejected by tenant-principal lookup.

### 2. Known local Workflow Card is dropped — FIND-TASK-001-2

**Violation and evidence.** REQ-004, INV-004, AC-011. The exact registered root
UID is available to `WorkflowCards::load`/`Workflow::run_on`, but
`workflow/local.rs:103–107` constructs `PublicWyrdGatewayCaller` without its
existing subject correlation. Gateway correctly captures a null Card. The
local V8 test asserts pair only; the hosted test cannot prove the local path.

**Recommendation.** Preserve the known registered root through existing shared
Workflow preparation and reuse `PublicWyrdGatewayCaller::with_subject` for the
Workflow Card and no application Run. Rust/Python/TypeScript share that owner.
Keep current server Card authorization: Workflow is already an eligible
observation target for the unbound caller in this journey. Do not loosen Card
scope, invent a transport/header, or assign a fake UID to unregistered authored
Workflows. Hosted attribution remains unchanged.

**Proof.** P3 strengthens the existing local direct/delegated calls to assert
pair, exact registered Workflow `card_uid`, null `run_id`, and actual provider
execution, then runs the focused hosted counterpart as a regression.

### 3. Missing observation owner disappears — FIND-TASK-001-4

**Violation and evidence.** REQ-008, AC-017. The nullable owner projection in
`wyrd-sql::lock_observation_bindings` returns a matching binding without a
principal. `verification/observations.rs:116–119` skips it without a diagnostic,
then commits and consumes the observation slice. No run/status or retry explains
the lost trigger. Inactive-owner resolution logging does not enter this branch.

**Recommendation.** Make the refusal observable at the existing binding-acceptance
owner using structured tracing with tenant, binding and owner Card. Consume this
nonretryable identity refusal while healthy sibling bindings continue. Reuse the
existing authority resolver for the other identity failures. No whole-batch retry,
new audit decision, failure table or shared-server failure is appropriate.

**Proof.** P4 includes an absent owner, no run/fallback/provider call, the captured
binding-specific diagnostic, and a healthy sibling that enqueues. Include the
missing SYSTEM and foreign-owner cases required by finding 7 at this same
acceptance boundary; do not duplicate execution guards downstream.

### 4. Credential completion audit loses actor/history — FIND-TASK-001-5

**Violation and evidence.** REQ-002/010, INV-001. The route correctly passes A/B
to key creation, but `components/auth/routes.rs:604–609` passes only effective
Principal A to `IssueApiKey::audit`. Its body at `wyrd-auth/src/issue_api_key.rs:149–172`
reconstructs A/A and omits the chain. The event identifying the issued key
contradicts its creation/permission record; an earlier correct audit row cannot
repair the false completion event.

**Recommendation.** Retain verified pair/history through this existing completion
audit owner, reusing the typed audit-delegation projection and the existing
credential-detail encoding convention. Preserve key ID, target, expiry,
resource/permission, direct issuance, secret redaction and nonblocking staging.
Update source-owned detail contracts and generated projections if needed; do
not parse tokens again, add another event/sink, or move issuance behavior.

**Proof.** P5 drives direct and delegated key issuance through the real HTTP
surface. Locate the retained completion event by key ID and compare its pair
and exact accepted chain with creation, checking that no key secret is retained.

### 5. Internal input reads lose accepted history — FIND-TASK-001-6

**Violation and evidence.** REQ-008/010 and AC-017. Drift passes only
`run.authority.principal` into `SystemReadAuthority::serving`
(`verification/drift.rs:896`, `authority.rs:172–177`); Eval strips the same
context at its reader construction (`eval.rs:151–156,565–582`). Oracle correctly
projects `AuthorizedQueryContext.delegation_chain`, but receives an empty chain.
The A/SYSTEM decision loses B and earlier accepted actors before either local
execution or peer serialization.

**Recommendation.** Preserve accepted history with the served subject at the
shared SYSTEM read-context producer, reusing the existing query-context chain
and serialization mechanisms. Reach both Drift and Eval readers. Keep execution
actor SYSTEM and exactly the existing table-UID-scoped internal grants. History
is accountability, never a permission source; do not use caller permissions,
refresh subject grants or create an alternate query authority.

**Proof.** P6 executes a delegated queued Drift read and inspects retained
A/SYSTEM decisions with the exact accepted chain. Add focused supporting proof
at the shared Eval context/serialization boundary, retaining its narrow grants
and no general caller input-read access. This tests a reachable successful read;
the rejected speculative scheduled-query object-denial proposal stays excluded.

### 6. Required negative and lifecycle proof is missing — FIND-TASK-001-7

**Violation and evidence.** AC-001/009/013/017/018; original Scenarios 1–3.
V1 stops at one delegated actor. V2 selects observation origins and covers a
suspended owner only. V3 stores real delegated manual Drift authority but both
fresh runtimes use `EngineScript`; no protected call consumes the reloaded
snapshot, and initiating-token expiry is not exercised. Green selector names,
unchanged JSON and realtime permission tests do not prove the missing seam.
Docs, changed support-desk examples and served OpenAPI proof are also absent
from the recorded checks.

**Recommendation.** Extend P7 with a second exchange to an actor that has a
permission excluded at an earlier hop; the real Verifier/judge/gateway must keep
the original subject, refuse the model, and send no provider request. P8 drives
a supported scheduled Drift occurrence through the real engine to a persisted
owner/SYSTEM result. P4 adds required identity rejection cases without changing
healthy binding processing.

Keep P9's public queue/retry/storage proof. P10 adds a focused integration seam
from an actual fresh queue claim into the existing protected gateway caller,
after grant edits and expiry of the initiating credential. A model the subject
or new grants permit but the captured intersection excludes must remain denied
without a provider request. Preserve history and A/SYSTEM attribution. Use
existing token-time controls/short-lived fixtures; no arbitrary sleep is a proof.
The public manual path supports Drift, which has no judge: its real journey plus
this protected-consumer seam is the selected proof. Do not add queued manual
Eval, scheduled Eval, a public API, fake permission evaluator, process-kill
harness or fixture framework to force the test.

Treat already-correct behavior as verification-only: a deliberately observed
passing branch does not justify production churn. Obtain missing docs/example/
served-contract evidence through the narrow existing commands below, not broad
journey suites. Record exact selectors and actual assertions/results.

### 7. Mandatory rustdoc is incomplete — FIND-TASK-001-8

**Violation and evidence.** `AGENTS.md` §16 and agent rules. Changed fallible
registration/replay/row-conversion operations omit substantive error/transaction
contracts; three new tests omit required panic contracts. Green Clippy does not
prove this explicit completion rule.

**Recommendation.** Correct the enumerated changed items in place:

- `wyrd-sql/src/queries/cards/register.rs`: `lookup_existing_operation`,
  `lookup_expired_operation`, `lookup_operation_by_id`,
  `insert_registration_operation`, `insert_card_row`.
- `wyrd-sql/src/row_types/cards.rs:122`: `ParsedCardRow::try_from`.
- `wyrd-server/src/components/cards/service.rs:636`: `replay`.
- New test items at `wyrd-runtime/src/request_context.rs:317`,
  `vala-sql/tests/integration/pg_olap_catalog.rs:84`, and
  `wyrd-sql/tests/integration/pg_cards_register.rs:239`.

Describe actual absent/conflict result meanings, failure conditions and
transaction/cancellation/partial-progress behavior where relevant, plus test
panic conditions. Keep implementations/assertions intact. Do not sweep untouched
items, add suppressions, create a permanent doc scanner, or open a style-only task.
Source comparison and formatting are sufficient proof for these prose changes.

### 8. Public result/peer descriptions contradict the contract — FIND-TASK-001-9

**Violation and evidence.** REQ-002/006. The results column description at
`docs/src/content/docs/understand/how-verification-works.md:167` and related
`architecture/wyrd-design.md:1264–1269` imply every result actor is SYSTEM.
Direct `VerificationControl::stage_result` preserves A/A or A/B; queued results
use subject/SYSTEM. A user filtering direct results by the documented SYSTEM
actor misses them. `architecture/bifrost-design.md:1406–1413` describes the
removed result-only peer attribution envelope, although all capture destinations
now carry a pair and audit keeps its own payload identity.

**Recommendation.** Correct those descriptions to distinguish direct results,
queued execution, and judge/input-read operations. Align peer and capture
transport-versus-payload prose with actual producers/consumers. Include r5's
approved creator and pre-identity refusal rules in the owning architecture.
Remove nearby contradictory old attribution prose and ephemeral AC references
while editing; no unrelated docs rewrite. Preserve production behavior rather
than changing it to match incorrect prose. Proof is source comparison plus the
docsite check; no new behavior test is required solely for wording.

## Preserved behavior and non-goals

Retain the original task's other obligations: permission intersection, original
subject and accepted authority, subject cost, immutable creators/replay authority,
current Workflow ownership, tenant separation, operation-bound span attribution,
Card/Verifier roles, reserved managed fields, Variant policy, subject sampling,
Operator rechecks and the prerequisite's trace/export behavior. Keep only the
single nonblocking Scribe outbox and its existing retry/loss guarantees.

No new migration, compatibility alias, token/sentinel privilege, identity
resolver, revocation mechanism, store, framework, feature, dependency or permanent
check. No authorization fallback to SYSTEM, ambient credentials, a guessed
administrator or full subject grants. General pre-identity bearer failures stay
unaudited; existing audited exchange refusals stay audited. The rejected
scheduled-query denial concern and additional SDK timeline copies are not work.

## Acceptance mapping

| Finding | Required closure under r5 |
|---|---|
| FIND-TASK-001-1 | AC-019: operator pair where verified; sentinel pair only for callerless creation; SYSTEM never self-subject; retries/reconcile preserve first pair across every listed owner/caller. |
| FIND-TASK-001-2 | AC-011: known local registered Workflow direct/delegated calls retain pair, exact Workflow Card and null application Run; hosted and Card authorization behavior remains. |
| FIND-TASK-001-3 | Superseded, not a code fix: r5 REQ-010/AC-015 is recorded and retained by the focused audit regression. |
| FIND-TASK-001-4 | AC-017: missing observation owner is refused observably, without fallback or loss of healthy siblings. |
| FIND-TASK-001-5 | REQ-010/AC-016: key completion audit agrees with creation pair and verified history, exposes no secret and stays nonblocking. |
| FIND-TASK-001-6 | REQ-008/010, AC-017: Drift/Eval input-read contexts and serialized audit retain accepted history and A/SYSTEM with unchanged narrow grants. |
| FIND-TASK-001-7 | AC-001/009/013/017/018: focused multi-hop, supported schedule, identity-refusal and recovered protected-authority/expiry proofs exercise actual boundaries; mandated missing scoped checks are recorded. |
| FIND-TASK-001-8 | Enumerated changed items and newly touched items satisfy required rustdoc; behavior/assertions and gates remain intact. |
| FIND-TASK-001-9 | Public/architecture attribution descriptions match direct/queued and peer contracts, including r5; docs check passes. |

## Focused proof and execution evidence

Run one scenario at a time: strengthen/add its focused proof, demonstrate RED
for the diagnosed gap, make the smallest correction, rerun GREEN and affected
previous proofs, then refactor only when it simplifies the same owner. Record
verification-only/static cases honestly. On failure rerun with `WYRD_LOG` or
`RUST_LOG` and read traces before diagnosis. Do not weaken assertions, add
suppression/ignore gates or change timing merely to get green.

Commands below run from repository root with repository-managed Postgres and
pinned toolchain. Cargo-backed commands run sequentially. **Planned** tests do
not exist yet: add them to the indicated existing target/module, confirm exact
selection before RED, and record any justified selector change. No new binary
or test framework is implied. Zero selected tests is not a pass.

### P1 — extend existing provisioning/reconciliation coverage

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=platform_admin_e2e::new_and_existing_tenants_receive_every_builtin)'"
```

### P2 — planned defining-SQL creator and idempotency proof

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --test integration -E 'test(=pg_migration::pg_tests::system_creation_uses_explicit_creator_or_sentinel)'"
```

### P3 — existing local Workflow and hosted regression

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=gateway_inference::loaded_workflow_calls_the_gateway_through_its_loading_client) | test(=gateway_inference::hosted_workflow_calls_record_the_caller_and_the_workflow_card)'"
```

### P4 — extend existing observation-owner journey with identity failures and diagnostics

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=eval_verification::background_owner_subject_system_actor)'"
```

### P5 — planned credential completion audit journey

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=audit_retention::issued_key_audit_keeps_delegation)'"
```

### P6 — planned real input-read/history journey and shared-context proof

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=verification_runtime::delegated_input_reads_keep_history)'"
```

### P7 — extend existing realtime authority journey with further delegation

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=eval_verification::realtime_subject_actor_authority)'"
```

### P8 — planned real scheduled Drift execution/result proof

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=verification_runtime::scheduled_owner_authority_is_retained)'"
```

### P9 — retain existing public queue/retry/storage proof

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=verification_runtime::queued_subject_authority_survives_restart)'"
```

### P10 — planned claimed-authority to protected-gateway integration seam

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=verification_runtime::reloaded_authority_cannot_widen_gateway_access)'"
```

### P11 — existing authenticated/platform audit and r5 pre-identity refusal regression

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=audit_retention::subject_actor_decisions_preserve_trusted_identity)'"
```

### P12 — existing served verification contract proof

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_openapi_contract::verification_contract_publishes_exactly_four_typed_operations)'"
```

### P13 — existing Rust journey executes the changed support-desk example code

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=support_desk::support_desk_answers_verifies_and_explains_every_request)'"
```

### P14 — existing Python and TypeScript support-desk equivalents

These tests import/execute the changed example code with the existing local
server/provider fixtures, as documented in `examples/support-desk/README.md`.
They close the example-proof gap without invoking a paid deployment. They are
focused examples, not full SDK journey suites. Prepare the owning native
bindings once when required, then execute the exact selected test:

```bash
mise run py:setup
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/test_support_desk.py::test_support_desk_answers_verifies_and_explains_every_request"
mise run ts:build
mise run ts:build:testing
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/support-desk.test.ts -t '^support desk answers, verifies, and explains every request$'"
```

### Narrow checks and completion record

For this Rust/SQL/shared-client/contract/docs write set, run:

```bash
mise run fmt
mise run lints
mise run check:deps
mise run check:tenant-isolation
mise run codegen:check
mise run docs:check
git diff --check
```

Every P1/P2 fixture starts/migrates an empty repository-managed database; retain
fresh-schema and seed assertions there. Do not replace that proof with a new
upgrade migration. P12 is the focused served OpenAPI proof for the changed
verification contract; codegen alone is insufficient. If credential detail
changes require another served contract assertion, add it to that existing
contract owner and record its exact selector.

Add only lanes demanded by the actual remediation write set: Python
format/lint/typecheck for changed Python or stubs; TypeScript format/lint/
typecheck for changed TypeScript or declarations; source-driven generator and
native declaration checks when those outputs change. A new supporting test's
exact focused command must be recorded and run. Do not add a broad `gate`,
`test:sql`, full principals integration lane, full SDK journeys, or full
capability suite to this remediation; cumulative broad proof runs once at
change review. Never run concurrent Cargo builds/tests in the shared target.

Append implementation evidence to this remediation task: actual execution
candidate, finding-to-source/proof mapping, expected RED and GREEN or justified
static/verification-only result, exact commands/counts, failure diagnoses,
remaining limits, and r5's supersession of FIND-TASK-001-3. Do not report test
names as proof of cases their assertions do not exercise. No production fix is
required for a proof-only case that already behaves correctly.

Keep the original reviewed source identities and reports intact. Submit the
new immutable candidate for `$wyrd-task-review`; implementation does not mark
its own findings closed or alter the historical verdict. Stop only if source
requires a new material decision outside r5; ordinary private implementation
choices and existing-owner adaptations remain implementation-owned.
