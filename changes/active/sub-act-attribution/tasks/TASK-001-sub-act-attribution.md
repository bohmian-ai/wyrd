---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-sub-act-attribution
spec_revision: 4
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009, AC-010, AC-011, AC-012, AC-013, AC-014, AC-015, AC-016, AC-017, AC-018]
depends_on: [SPEC-verify-attribution@3]
parent_task:
remediates: []
---

# TASK-001 Consistent subject and actor attribution

## Outcome and Value

Every in-scope work record says who the work serves (`sub_principal_id`) and
who performs it (`act_principal_id`). The original subject and accepted
effective authority survive realtime request → Verifier → judge → gateway,
queueing, retries, and authenticated internal forwarding. Gateway permission,
admission, and cost remain bound to that subject. Stored rows and all public
projections agree, without legacy aliases or a parallel attribution pipeline.

This one task owns every REQ, INV, and AC of approved spec revision 4. Mixed
old/new schemas and authority semantics are not independently deliverable.
Execute through `$wyrd-implement`; private implementation choices remain with
that workflow. Planning does not authorize implementation or declare readiness.

**Dependency.** Start from integrated verify-attribution r3, preserving trace
continuity, linked-ingest tenant eligibility, bounded/non-blocking tenant span
export, timeline reads, and the queued run's enqueuing context. Its packet is
currently active; approval or the presence of its code alone does not establish
completion. Confirm its integrated evidence before starting dependent work.
Do not duplicate or reimplement that change here.

## Owners, Scope, Consumers, and Prohibited Changes

Use existing owners end to end:

- `wyrd-spec` owns IO-free persisted/wire value contracts. `wyrd-runtime` and
  authentication retain the existing effective principal/permission evaluator.
  Server `Caller` and trusted ingest contexts carry verified identity; the
  server owns authority capture and execution.
- `VerificationControl`, `DirectExecutor`, `VerifierRunQueue`,
  `VerificationScheduler`, `VerifierRunner`, and `ServerWyrdGatewayCaller`
  remain the execution path. Extend accepted contexts and the existing queue;
  do not create another executor, snapshot service, permission engine, or
  delegated-token issuance path.
- SQL owners update the defining migrations, queries, constraints, and views
  for the spec's named tables. Bifrost Gate/Scribe/table owners update managed
  contracts, stamping, validation, peer forwarding, and subject-based sampling.
- `ScribeOutbox` remains the sole server-internal write path. Audit payload
  identity is distinct from the outbox's transport identity. Tenant-span
  routing remains owned by `wyrd-telemetry`, with identity bound to the
  emitting operation rather than inferred from its trace parent.
- Shared `wyrd-client` remains the client implementation; Rust, Python, and
  TypeScript SDKs project it. Update HTTP/MCP/CLI/UI consumers that actually
  expose changed fields, authored Python stubs, generators, examples, and
  fixtures in the same task. Do not duplicate server behavior in an SDK.

**Fixed behavior.** Direct A records A/A; B for A records A/B; background
binding work records owner/SYSTEM; judge execution records original
subject/SYSTEM while retaining the originating effective authority and audit
history. Workflow steps retain their captured subject/actor, Workflow Card,
and null application `run_id`. A downstream delegation can only narrow
authority. Permissions use the existing intersection semantics, never union.
Neither SYSTEM nor the Verifier/model/transport identity can replace `sub`.

Accepted manual authority survives durable claim/retry/restart without tokens
or credentials. Background acceptance resolves active owner/SYSTEM in the run
tenant and captures the owner's effective authority. Accepted authority follows
the existing Workflow snapshot lifetime: grant edits/token expiry affect new
acceptance, not the accepted run. Every protected downstream action still checks
its own resource/permission. Missing trusted identity/authority fails closed.
Preserve the existing narrow, tokenless Verifier input-read capability; its
attribution becomes run-subject/SYSTEM, without granting the caller input access.

Registration/manual-run replay scopes include tenant, subject, actor, and key.
Preserve exact-replay/conflicting-body behavior and admission to replay/read;
replay never changes accepted attribution or authority. Preserve Workflow's
subject-based idempotency and background binding/source/occurrence deduplication.
Creation pairs remain immutable across later entity updates.

**Exclusions.** Preserve identity/ownership keys, Variant payload policy,
engine lineage, run-linked `operator_dispatches`, Operator authority rechecks,
Workflow lifetime/idempotency, token issuance/exchange, roles, and tracing
semantics. No new dependency, Cargo feature, compatibility field/view, backfill,
schema-evolution mechanism, or migration is justified by this task. Do not add
a generic context framework, global registry, or second writer. Keep all seven
spec invariants, existing tenant/Card authorization, RLS transaction ownership,
and blocking-permission/non-blocking-audit behavior.

## Reuse Map

Paths below are repository-relative source evidence, not an implementation
allowlist. New private helpers remain implementation-owned.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Verified authority and actor | `wyrd-auth-verify/src/lib.rs::VerifiedToken`; server `components/auth/caller_extractor.rs::Caller::from_authenticated` | `principal_extractor.rs::AuthenticatedPrincipal` and its conversion to `Principal`; gateway `workflow.rs::ServerWyrdGatewayCaller::call` | Conversion to bare principal loses actor history; execution actor and original delegated authority need distinct meanings | Carry the existing verified effective principal/history through existing trusted contexts; last flattened actor is current | None; no token or permission evaluator changes |
| Direct/judge execution | server `components/verification/service.rs::VerificationControl::execute`, `verification/direct.rs::DirectExecutor`; `components/gateway/workflow.rs::ServerWyrdGatewayCaller` | Existing `eval_verification` journey module; SDK gateway and timeline journeys | Judge work must use original subject/effective authority with SYSTEM execution attribution | Extend the existing Verifier-to-gateway path, including Task and Eval judge fan-out; keep `GatewayInvocation` checks | None; gateway already evaluates captured caller authority |
| Durable and background runs | `wyrd-sql/src/queries/verifier_runs.rs::VerifierRunQueue::enqueue/enqueue_manual`; server `verification/runner.rs::VerifierRunner`, `scheduler.rs::VerificationScheduler` | `pg_verifier_runs` integration module; `eval_verification` and `verification_runtime` journey modules | Queue records carry requester identity but not complete accepted authority; old background identity semantics | Extend the existing run record, shared enqueue, claim, and execution paths; preserve leases/fencing/origin deduplication | Persistence of accepted authority is required; a separate store/queue is not |
| Creation and replay | `wyrd-sql/src/queries/cards/register.rs::NewRegistrationOperation`, `NewCardRow`; existing auth/connection queries; Vala table catalog | `pg_cards_register`, `pg_verifier_runs`; SDK registration consumers | Old creator/writer fields and requester-only replay scope | Change existing defining schemas and query owners; include both identities in locks/lookups/uniqueness | None; preserve existing replay owners and request digests |
| Ingest and managed arrays | `vala-bifrost-redux/src/gate/auth.rs::AuthContext`, `gate/mod.rs::dispatch_canonical`, `contracts.rs::ScribeIngressFrame`, `scribe/execution_lanes.rs::managed_arrays` | `scribe/write_read.rs` correlated writes; `server/verification_runtime.rs` typed payloads; SDK Bifrost journeys | Gate currently passes only principal to Scribe; arrays stamp one managed `principal_id` | Carry trusted pair through existing HTTP/gRPC/peer frame paths; update managed constants/reservation and schema fingerprints together | None; public row/header values cannot establish attribution |
| Sampling | `vala-bifrost-redux/src/scribe/claim_sample.rs::ClaimVariantSample` | Existing per-writer-stratum tests in that module | Managed identity name and selected attribution axis | Use managed subject as stratum identity; keep existing algorithm and Variant payload boundary | None |
| Audit/capture/results | server `scribe_outbox.rs::ScribeWrite`, `ScribeBatch::into_frame`, `VerifierAttribution`; auth `jwt_bearer.rs`, `platform_authz.rs`; server `audit/mod.rs` | `server/audit_retention.rs`; existing outbox retry/failure tests | Payload identity can differ from transport sentinel; nil/unresolved audit convention and old result/Card fields | Extend current payloads, peer projections, and outbox batching; use existing unresolved sentinel; retain real platform principals under reserved audit tenant | None; no second audit sink or synchronous write |
| Server spans | `wyrd-telemetry/src/export.rs::TenantSpanRoute`; server `AuthenticatedPrincipal::from_verified`, `scribe_outbox.rs::ServerSpanSink` | SDK action timelines; MCP traced verification | Single routed principal cannot express request A/B versus judge A/SYSTEM | Extend existing trusted operation route and span staging while preserving prerequisite sampling/eligibility | None; never route from client identity headers or trace IDs |
| Workflow calls | server `components/workflow/host.rs`, `runs.rs::WorkflowRuns`; `ServerWyrdGatewayCaller` | Rust SDK `gateway_inference.rs` loaded-Workflow journey | Captured caller pair and Workflow Card must reach captured gateway row | Extend existing captured caller/correlation projection; retain run ownership and in-memory lifecycle | None |
| Public consumption | `wyrd-client` Bifrost/cards/observe/workflow facades; SDK binding roots and authored/generated row projections | Rust `action_timeline.rs`, `query_bifrost.rs`; Python action timeline/Bifrost tests; TS realtime/Bifrost tests | Public rows/read queries/constants expose obsolete names | Change owning contracts and their existing projections; regenerate source-driven artifacts | None; no language-specific durable contract |
| SYSTEM provisioning | `wyrd-dev-fixtures/src/pg.rs`; existing server tenant provisioning | `WyrdTestServer` and `VerificationFixture` provisioning used by owning journeys | Every seeded tenant must have an active tenant SYSTEM just as production does | Extend current seed/provisioning owner and prove actual records through its existing fixture | None; no SYSTEM token or new identity kind |

## Approach

1. Confirm the dependency and revalidate this map against the integrated source.
   Classify remaining old-name occurrences by the spec's table/policy inventory.
2. Prove realtime subject/authority propagation using existing authentication,
   Verifier, and gateway owners. Separate execution actor from accepted authority.
3. Extend existing queue acceptance/persistence and background identity resolution;
   prove restart, denial, tenant isolation, and idempotency without new orchestration.
4. Change defining SQL/Bifrost schemas and every writer/read projection together,
   including outbox batching/peer contracts, Card-role fields, audit, and spans.
5. Close shared-client and SDK consumers, examples, generated contracts, and
   architecture authorities; delete old attribution fields/branches/conversions
   once their callers move. No transitional aliases or unused fallback paths.
6. Run focused scenario commands and applicable owner/static checks. Record
   requirement-to-evidence closure, without duplicate final aggregate runs.

## Ordered Implementation Scenarios

Commands referenced as V1–V11 appear in Verification and Evidence. New selectors
are explicitly planned additions to inspected existing targets/modules. Keep
tests in those established owners; reuse fixtures and parameterize cases where
it makes the behavior clearer. Test names do not prescribe private fixture code.

### Scenario 1 — Realtime subject and authority reach the gateway

**Behavior.** Direct A and delegated B-for-A traverse request → Task/Eval
Verifier → judge → gateway. Acceptance records A/A or A/B; judge execution,
gateway rows, and judge-operation spans record A/SYSTEM. Required permission
and model restrictions remain the accepted effective intersection, accounting
charges A, and audit retains delegation history. If only A or only B permits
the model, refuse without an upstream provider request; further delegation
cannot restore excluded authority. Missing trusted context fails closed.
Covers REQ-001–004/011, INV-001–006, AC-003/004/010/012/018.

**RED.** Add the planned `eval_verification::realtime_subject_actor_authority`
journey (V1), covering success and both asymmetric denials. Add a focused
in-owner negative test for missing trusted context, recording its actual exact
command before execution. Expected RED is lost/wrong actor or widened judge
authority, not a provider/setup failure. Include existing Card-scope checks.

**GREEN.** Extend existing accepted caller and Verifier/gateway execution
contexts; keep `GatewayInvocation` authorization/admission. Update the schemas
and projections necessary to observe this path coherently. Preserve original
effective authority when execution changes actor to SYSTEM. Never rebuild it
from subject ID or actor history.

**REFACTOR.** Remove duplicate pair construction and authority-dropping
conversions where their callers have moved; retain the existing trusted boundary
and evaluator. Rerun V1 after each simplification.

### Scenario 2 — Background runs serve their binding owner

**Behavior.** Scheduled and observation-triggered runs, judge calls, and
results record owner/SYSTEM. A different observation writer supplies no run
authority. Resolve active owner/SYSTEM in the run tenant at acceptance;
missing/inactive/foreign-tenant identities cannot fall back. Deny a judge model
outside owner authority and account permitted work to the owner. Preserve narrow
internal input reads and their resource restrictions. Two bindings sharing
SYSTEM remain separate. Covers REQ-002/003/007/008, INV-001–006, AC-001/002/008/017.

**RED.** Add `eval_verification::background_owner_subject_system_actor` (V2)
in the existing real-server journey module, covering scheduled and observation
origins and negative identity/permission cases. Expected RED is writer/SYSTEM
substitution, absent seeded SYSTEM, or incorrect owner accounting/authority.
Extend existing source-linked observation scenarios rather than keeping old
writer-as-requester assertions alongside the new rule.

**GREEN.** Extend existing binding enqueue/scheduler/provisioning owners to
resolve and capture owner authority once at acceptance. Preserve input-read
capability checks, occurrence/source deduplication, and source-record links.
Run V1 and V2.

**REFACTOR.** Delete replaced origin-specific requester fallbacks and share
the existing enqueue path. Do not generalize all background services.

### Scenario 3 — Accepted authority survives durable execution

**Behavior.** A delegated accepted manual run retains original subject,
originating actor/history, narrowed permissions, and resource restrictions
through claim, retry, and worker restart. Later grants/token expiry do not
replace the accepted snapshot. Every judge still checks its model permission;
reclaim never reloads unrestricted subject grants. No credential or authority
snapshot appears in public views. Covers REQ-008/011, INV-001/003/005–007,
AC-012/013/017/018.

**RED.** Add `verification_runtime::queued_subject_authority_survives_restart`
(V3) to the existing real-server runtime journey owner. Exercise durable
reclaim with a newly created worker/context, not retained caller memory; use
the existing repository process harness if process termination is required.
Expected RED is loss of authority/history or permission widening after reload.
Use focused SQL integration coverage for complete stored snapshot round-trip
and tenant-bound retrieval where a journey would obscure that seam.

**GREEN.** Extend existing run acceptance, stored run/claim projection, and
runner consumption. Keep snapshots server-only and credential-free. Preserve
leases, fencing, cancellation, bounded retries, and audit behavior. Run V1–V3.

**REFACTOR.** Reuse existing principal/permission serialization and captured
caller patterns; remove requester-only reconstruction instead of retaining it
as a fallback. Do not add a capability store or snapshot renewal workflow.

### Scenario 4 — Creation and replay retain both identities

**Behavior.** Named SQL creation records receive a non-null verified pair,
which updates cannot rewrite. Registration and manual verification separate
same actor/different subject and same subject/different actor, while exact
replay reuses the operation and changed input conflicts. Concurrent retry
serialization uses the same scope as lookup/uniqueness. Workflow ownership and
background origin deduplication remain unchanged. Covers REQ-001/002/005/009,
INV-001/003/007, AC-006/007/008/014/016.

**RED.** Add planned SQL tests
`pg_cards_register::subject_actor_creation_and_replay` and
`pg_verifier_runs::subject_actor_manual_replay` (V4), extending their existing
integration owners. Include current migrations from an empty database and
concurrent duplicate/conflicting requests. Add the public replay cases to V1's
real SDK/server flow so SQL-only proof does not substitute for user behavior.
Expected RED is old columns, mutation of creation pair, or cross-identity replay.

**GREEN.** Update existing queries, advisory lock scope, unique constraints,
and stored responses in their defining migrations. Fold/remove the specified
unshipped migration while preserving its enqueue trace-context column. Keep
existing client key derivation and Workflow idempotency. Run V1–V4.

**REFACTOR.** Remove obsolete creator/requester fields, query arguments, and
decoding branches; use name-based projection. Do not invent generic repository
or idempotency abstractions.

### Scenario 5 — Ingest stamps the pair and rejects spoofing

**Behavior.** HTTP/gRPC, SDK-managed writes, and authenticated peer forwarding
preserve trusted direct/delegated attribution. Observation-managed schemas
reserve both new names and reject client-supplied values with the existing
reserved-column error. Keep Card scope and tenant isolation; Variant payload
policy is unchanged. Managed sampling uses subject identity without changing
its algorithm. Covers REQ-001/002/005/006, INV-001/003–006, AC-005/006/016.

**RED.** Add planned `query_bifrost::subject_actor_rows_and_reserved_columns`
(V5) through the public Rust SDK, and the Python/TypeScript counterparts in V9.
Cover write → flush/shutdown → read, direct/delegated identities, both reserved
names, and tenant/Card-scope refusals. Exercise the existing HTTP/gRPC ingress
and peer paths in their owning integration tests; record exact commands for
any additions. Expected RED is the old managed column or a lost actor at Gate.

**GREEN.** Extend existing auth/frame/stamping/table owners and managed-column
constants. Update outbox batching/peer projections so slices preserve the
correct pair and never combine rows under another operation's identity.
Keep source-driven schemas/fingerprints and the existing policy boundary.
Run earlier scenarios and V5/V9 as each runtime becomes available.

**REFACTOR.** Remove the old managed principal stamping branch and outdated
schema expectations. Reuse existing reserved-name validation and sampling.

### Scenario 6 — Audit distinguishes identity from transport

**Behavior.** Allowed and denied authenticated decisions retain actual
subject/actor/history. Platform audit stays under the reserved audit tenant
with real platform identities. Authentication refusals use the existing
PLATFORM_AUDIT_PRINCIPAL pair and unresolved detail, never nil or rejected
claims. Outbox transport sentinels cannot replace payload identities. Audit
failure remains non-blocking. Covers REQ-001/002/005/010, INV-001–006,
AC-003/004/015/016.

**RED.** Add `audit_retention::subject_actor_decisions_preserve_trusted_identity`
(V6), using the existing retained-audit journey. Drive tenant/platform allowed
and denied requests and authentication refusals through HTTP; include missing,
invalid, and misleading credential claims. Extend existing outbox failure
coverage rather than building a second failure harness. Expected RED is an
old field, nil/untrusted payload identity, or transport-sentinel substitution.

**GREEN.** Change existing audit builders, row schema, subject-kind field,
staging, and peer encoding together. Preserve one event per permission check
and unchanged loss/retry/accountability behavior. Run V1 and V6 plus earlier
affected scenarios.

**REFACTOR.** Delete stale nil-principal builders and old attribution aliases;
reuse the current unresolved sentinel convention. No audit service replacement.

### Scenario 7 — Existing public journeys expose the same contract

**Behavior.** All first-class SDK typed rows, reads, errors, and trace timelines
reflect the pair and Card roles; request spans carry A/A or A/B and judge spans
carry A/SYSTEM. Workflow gateway calls carry captured caller/actor, Workflow
Card, and null application run ID. MCP discovery → act → observe uses the same
verified authority and row contract. Covers REQ-001–006/010/011, INV-001–007,
AC-003–006/009–012/015/016/018.

**RED.** Extend the existing Rust/Python/TypeScript timeline journeys (V7/V10)
and Rust loaded-Workflow journey (V8), adding delegated cases and assertions
on returned rows/audit as appropriate. Extend the existing MCP verification
journey (V11) with the same verified-subject/delegation assertions. Expected
RED is a stale field, dropped actor, wrong Card role, or mismatched operation
span attribution. Do not duplicate the prerequisite's trace-export tests.

**GREEN.** Update shared contracts/client projections and language wrappers,
authored Python stubs, MCP catalog projections, and consumed generated output.
Preserve established transport and Workflow semantics. Run V1–V11.

**REFACTOR.** Remove obsolete query joins, fixtures, examples, declarations,
documentation, and alternate attribution logic. Retain legitimate identity
keys and historical change records with an explicit disposition.

## Acceptance Criteria

In addition to every mapped spec invariant and exclusion:

| Spec criterion | Accountable owner | Required proof |
|---|---|---|
| AC-001/002 | Binding enqueue, scheduler, runtime, gateway | V2: schedule/observation owner/SYSTEM, different writer, owner denial/cost |
| AC-003/004 | Auth, gateway, verification, audit, SDK projections | V1/V6/V7/V10: direct/delegated rows and matching decision identity |
| AC-005 | Managed schema/ingest and SDK boundaries | V5/V9: both reserved names refused in each SDK |
| AC-006 | Every inventory owner and consumer | Classified old-name disposition; generators and typed projections agree |
| AC-007 | SQL/schema owners | No new migration; specified migration removed/folded; `test:sql` fresh-schema proof |
| AC-008 | Existing tenant provisioning/dev fixtures | V2 checks active SYSTEM on actually seeded tenants |
| AC-009 | Implementer/change review | Focused commands plus affected owner checks; integrated gates at change review |
| AC-010 | Trusted span routing, outbox, SDK timeline owners | V7/V10/V11: request versus judge pair, unchanged timeline and Card join |
| AC-011 | Workflow captured caller/gateway | V8: direct/delegated captured pair, Workflow Card, null application run ID |
| AC-012/018 | Direct/Task/Eval judge/gateway path | V1: original subject, intersection denials, no ambient/SYSTEM fallback |
| AC-013 | Durable run acceptance/claim/runtime | V3: accepted authority/history survives reclaim/restart and grant changes |
| AC-014 | Registration/manual queue replay owners | V4 and public V1 cases: identity scopes, concurrency, replay/conflict, immutable capture |
| AC-015 | Tenant/platform auth/audit and MCP boundary | V6/V11; reuse outbox non-blocking-failure proof |
| AC-016 | Full schema/writer/reader inventory | Scenarios 4–7; both ingress transports/peer projection; subject sampling |
| AC-017 | Tenant identity, queue/input-read/Operator owners | V2/V3 and existing affected regressions: missing identities fail, tenant isolation and narrow reads/rechecks preserved |

## Expected Write Set and Consumer Closure

- `crates/wyrd-spec` attribution-bearing observation/gateway/verification/
  audit/run/registration contracts and managed-column declarations.
- `crates/shared/wyrd-runtime`, `wyrd-auth-verify`, `wyrd-telemetry`, and server
  auth context as required for trusted pair/authority propagation. Preserve
  token verification/issuance semantics; avoid changes where projection suffices.
- `crates/wyrd/wyrd-server` auth, verification, Workflow correlation, gateway
  accounting/capture, audit, Scribe outbox, internal peer/capture handlers,
  and `crates/wyrd/wyrd-tonic` peer contracts where they carry old fields.
- `crates/wyrd/wyrd-sql` and `crates/vala/vala-sql` named defining migrations,
  queries, views, constraints, and affected SQL metadata; dev-fixture seeds.
- `crates/vala/vala-bifrost-redux` Gate/Scribe contexts, managed arrays,
  schemas, sampling, reserved names, table registration, and affected readers.
- Shared `wyrd-client`, three SDK projections/stubs/declarations/tests, MCP/CLI/
  UI exposed-field consumers, generators and examples affected by the rename.
- Existing Rust/SQL/SDK/MCP test owners identified above; no new test binary
  or fixture framework is justified.
- `architecture/wyrd-design.md`, `bifrost-design.md`, and
  `wyrd-security-posture.md`: replace superseded attribution/result-token prose,
  preserve prerequisite trace behavior, document approved authority boundaries.

This is guidance, not a file allowlist. Remove stale logic as its callers move;
do not change unrelated principal identity/owner fields or preserve aliases.

## Verification and Evidence

### Exact focused commands

Run from repository root, sequentially for Cargo-backed work. Commands use
inspected existing manifests/targets/module paths and the repository Postgres
wrapper. **Planned** names become selectable when their tests are added; they
are not claims that tests exist today. Confirm actual selectors with nextest
listing before RED; zero selected tests is not evidence. New supporting tests
also require their exact command in the implementation record. On failure,
rerun with `WYRD_LOG`/`RUST_LOG` and inspect traces before diagnosis.

V1 — planned realtime journey:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=eval_verification::realtime_subject_actor_authority)'"
```

V2 — planned background journey:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=eval_verification::background_owner_subject_system_actor)'"
```

V3 — planned durable runtime journey:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=verification_runtime::queued_subject_authority_survives_restart)'"
```

V4 — planned SQL additions in the existing integration modules:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --test integration -E 'test(=pg_cards_register::subject_actor_creation_and_replay) | test(=pg_verifier_runs::subject_actor_manual_replay)'"
```

V5 — planned public Rust SDK managed-row journey:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=query_bifrost::subject_actor_rows_and_reserved_columns)'"
```

V6 — planned retained-audit journey:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=audit_retention::subject_actor_decisions_preserve_trusted_identity)'"
```

V7 — extend the existing Rust SDK action timeline:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=action_timeline::service_action_timeline_is_one_connected_tree)'"
```

V8 — extend the existing public Workflow/gateway journey:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=gateway_inference::loaded_workflow_calls_the_gateway_through_its_loading_client)'"
```

V9 — planned Python/TypeScript managed-row counterparts; build their existing
native bindings once before focused runs:

```bash
mise run py:setup
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/bifrost/test_bifrost_e2e.py::test_subject_actor_rows_and_reserved_columns'
mise run ts:build
mise run ts:build:testing
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/bifrost-write.test.ts -t "^subject actor rows and reserved columns$"'
```

V10 — extend existing Python/TypeScript action timelines, reusing those builds:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/test_action_timeline.py::test_service_action_timeline_is_one_connected_tree'
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/verify-in-real-time.test.ts -t "^service action timeline is one connected tree$"'
```

V11 — extend the existing real MCP verification journey:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=verification::pg_tests::verification_tool_continues_trace_context)'"
```

### Owner checks and static closure

Use the smallest applicable owner/unit/regression lanes for the actual write
set, including fresh SQL (`mise run test:sql`), format/lints (`mise run fmt`,
`mise run lints`), dependency/tenancy (`mise run check:deps`,
`mise run check:tenant-isolation`), and `mise run codegen:check`. Changed Python
stubs/exports require `py:format`, `py:lints`, and `py:typecheck`; changed TS
projections require `ts:format:check`, `ts:lints`, `ts:typecheck`, and
`ts:napi:check` when generated native declarations change. Add owning MCP
catalog/unit proof and served OpenAPI proof when those surfaces change;
codegen does not establish either runtime contract. Select their existing
owner lanes from `mise.toml`, not a new broad test task.

Do not list/run whole journey suites or the repository aggregate as this
task's iteration proof; integrated change review owns complete affected
capability gates and all-language suite coverage once. In particular,
`test:gateway:gate` omits SDK gateway journeys; V1/V7/V8/V10/V11 cover the
affected public flows here and do not excuse integrated SDK coverage later.

Documentation, generated output, renamed declarations, and old-name
dispositions use static proof, not manufactured RED tests. Regenerate from
source; edit authored Python stubs directly. Classify retained identity keys,
lineage, and historical records, and verify there is no old live attribution
branch/alias. Record the inventory, expected RED, GREEN, actual exact commands,
enabled features, AC/INV evidence, and `git diff --check` result in the task
execution record. Do not add a permanent repository check for this one-time
rename inventory. Test selection/build cost alone does not justify weaker proof.

## Material Stop Conditions

- The integrated prerequisite lacks required r3 behavior/evidence. Resolve it
  in its owning packet before dependent work; do not duplicate it here.
- Evidence contradicts the approved subject/actor, accepted-authority lifetime,
  delegation intersection, platform/unresolved audit, table-policy inventory,
  or idempotency decisions. Route semantic changes through `$wyrd-spec`.
- Previously shipped data or consumers require a migration, compatibility
  surface, or evolution strategy contrary to the approved unshipped baseline.
- A previously unclassified work-record owner expands the inventory or requires
  a new public/persisted/security boundary. Do not silently blanket-rename it.
- Existing owners cannot preserve trusted authority across a required boundary
  without a materially different architecture. Document the concrete gap before
  introducing replacement orchestration or widening privileges.

Private helper layout, serialization within approved owner contracts, focused
fixture extension, and equivalent exact command repairs are implementation
choices, not reasons to stop or request approval. Existing correct behavior
may use regression/no-op proof; do not create churn to manufacture RED.

## Authority Links

- [Approved spec r4](../spec.md)
- [Prerequisite spec r3](../../verify-attribution/spec.md)
- [AGENTS.md](../../../../AGENTS.md)
- [Agent rules](../../../../architecture/agent-rules.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Wyrd doctrine](../../../../architecture/wyrd-doctrine.mdx)
- [Bifrost design](../../../../architecture/bifrost-design.md)
- [Security posture](../../../../architecture/wyrd-security-posture.md)
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md)
- [Implementation execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../../architecture/references/languages/testing-workflows.md)

## Implementation Evidence

Commits `d917a2751..HEAD` on `change/sub-act-attribution` (prerequisite
`split/verify-attribution` integrated at `c7ec3ef91`). Features: default
per-target features; `mise run lints` and the type-check lanes use
`--all-features`.

### Acceptance

| Criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-001/002, AC-008 | `wyrd-sql` `verifier_runs.rs` enqueue owner/SYSTEM; server `scheduler.rs`, `runner.rs`; `wyrd-dev-fixtures/src/pg.rs` seeds SYSTEM | V2 `eval_verification::background_owner_subject_system_actor` | PASS |
| AC-003/004 | `caller_extractor.rs` keeps the verified pair; `audit/mod.rs`, `scribe_outbox.rs` stage it as content; SDK row projections | V1, V5, V6 `audit_retention::subject_actor_decisions_preserve_trusted_identity`, V7, V9, V10 | PASS |
| AC-005 | Reserved names in `vala-bifrost-redux` managed columns and the `wyrd-client`/`wyrd-queue` row and table checks; `RegisterTableRequest.fields` doc | V5, V9 Python `test_subject_actor_rows_and_reserved_columns`, V9 TS `subject actor rows and reserved columns` | PASS |
| AC-006 | Old names removed from schemas, wire types, SDKs, generators, examples, architecture | Disposition inventory below; `mise run codegen:check` | PASS |
| AC-007 | Defining migrations edited; `20261014000001_verifier_run_requester_every_origin.sql` deleted; no migration added in `b22718f2c..HEAD` | `mise run test:sql` | PASS |
| AC-009 | Focused commands below plus owner/static lanes | This record | PASS |
| AC-010 | `wyrd-telemetry` `TenantSpanRoute`/`TenantRoutes::rebind`; server span staging under the operation pair | V7 Rust, V10 Python/TS, V11 MCP: request A/A (A/B delegated), judge gateway spans A/SYSTEM | PASS |
| AC-011 | `components/workflow/host.rs` attributes hosted steps with the Workflow Card and no Run | V8 `gateway_inference::loaded_workflow_calls_the_gateway_through_its_loading_client`, `gateway_inference::hosted_workflow_calls_record_the_caller_and_the_workflow_card` | PASS |
| AC-012/018 | `verification/direct.rs`, `service.rs`, `ServerWyrdGatewayCaller` carry subject and delegated intersection to the judge | V1 `eval_verification::realtime_subject_actor_authority`; TS `judge call is attributed to the run and card` | PASS |
| AC-013 | Accepted authority frozen on `wyrd.verifier_runs` and rebuilt on claim | V3 `verification_runtime::queued_subject_authority_survives_restart` | PASS |
| AC-014 | Pair-scoped idempotency in `cards/register.rs` and `verifier_runs.rs` | V4 `pg_cards_register::subject_actor_creation_and_replay`, `pg_verifier_runs::subject_actor_manual_replay` | PASS |
| AC-015 | Audit stages verified pair; refusals before resolution record nothing; platform pairs retained | V6; `platform_admin_e2e::an_operator_configures_and_removes_federated_platform_sign_in`; `auth::jwt_bearer::pg_tests::wrong_audience_is_rejected_and_audited`; V11 | PASS |
| AC-016 | Gate `AuthContext` pair through HTTP/gRPC/peer frames; `claim_sample.rs` strata on subject | V5/V9 rows; `test:sql`; V7/V10/V11 spans | PASS |
| AC-017 | Missing identities fail at enqueue/claim; tenant isolation preserved | V2, V3; `mise run check:tenant-isolation` | PASS |

Non-goals stayed out of scope: no token/permission evaluator changes, no new
store, queue, audit sink, migration, alias, or compatibility path.

### Exact commands

Postgres-backed commands ran under
`scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && …"`.

```bash
mise exec -- cargo nextest run --locked -p wyrd-sql --test integration -E 'test(=pg_cards_register::subject_actor_creation_and_replay) | test(=pg_verifier_runs::subject_actor_manual_replay)'   # 2 passed
mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=eval_verification::realtime_subject_actor_authority) | test(=eval_verification::background_owner_subject_system_actor) | test(=verification_runtime::queued_subject_authority_survives_restart)'   # 3 passed
mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=audit_retention::subject_actor_decisions_preserve_trusted_identity)'   # 1 passed
mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=query_bifrost::subject_actor_rows_and_reserved_columns) | test(=gateway_inference::loaded_workflow_calls_the_gateway_through_its_loading_client) | test(=gateway_inference::hosted_workflow_calls_record_the_caller_and_the_workflow_card) | test(=action_timeline::service_action_timeline_is_one_connected_tree)'   # 4 passed
mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=verification::pg_tests::verification_tool_continues_trace_context)'   # 1 passed
mise exec -- cargo nextest run --locked -p wyrd-server --test integration --run-ignored=all -E 'binary_id(wyrd-server::integration) & (test(/pg_workflow_runs::/) | test(/workflow/))'   # 10 passed
mise exec -- cargo nextest run --locked -p wyrd-server --test integration --run-ignored=all -E 'test(=platform_admin_e2e::an_operator_configures_and_removes_federated_platform_sign_in)'   # 1 passed
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/workflow/) | test(/gateway::/) | test(=auth::jwt_bearer::pg_tests::wrong_audience_is_rejected_and_audited)'   # 57 passed
cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/bifrost/test_bifrost_e2e.py::test_subject_actor_rows_and_reserved_columns tests/integration/test_action_timeline.py::test_service_action_timeline_is_one_connected_tree   # passed
cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/bifrost-write.test.ts -t "^subject actor rows and reserved columns"   # passed
cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/verify-in-real-time.test.ts -t "^(service action timeline is one connected tree|judge call is attributed to the run and card)$"   # passed
```

Owner and static lanes: `mise run test:sql`, `mise run fmt`, `mise run lints`,
`mise exec -- cargo check --locked --workspace --all-targets`,
`mise run codegen:check`, `mise run check:deps`,
`mise run check:tenant-isolation`, `mise run py:format`, `mise run py:lints`,
`mise run py:typecheck`, `mise run ts:format:check`, `mise run ts:lints`,
`mise run ts:typecheck` — all pass. `git diff --check main...HEAD` is clean
after trimming two blank-at-EOF lines inherited in the prerequisite's review
records.

### Diagnoses

- **Audit fingerprint.** Symptom: audit_log registration fingerprint mismatch.
  Cause: `source_schema_fingerprint` dropped sub/act for every table, but
  audit_log carries them as content. Fix site: the fingerprint owner keeps the
  pair for content-attributed tables (`517b49438`).
- **V2 owner denial.** Symptom: owner B's background judge call was allowed.
  Cause: a registered Service principal holds the built-in workload role,
  which grants gateway invoke. Fix site: test setup removes B's owner roles
  (V2) and A's default Card role (V1); production unchanged.
- **V5 client reservation.** Symptom: the client accepted a
  `sub_principal_id` column. Cause: the `wyrd-queue` reserved-name copy lacked
  the pair (the client tier cannot depend on `vala-bifrost-redux`). Fix site:
  that copy, with a unit assertion (`85a9f49f5`).
- **V6 refusals.** Symptoms: `not-a-token` returned 400, and the delegated
  user actor failed. Causes: a malformed bearer is documented
  `WYRD_AUTH_400_BAD_TOKEN_FORMAT`; a forged-but-well-formed token needs a real
  signature; RFC 8693 delegation requires a Card-bound actor
  (`actor_not_card_bound`). Fix site: test expectations and setup.
- **V8 Workflow Card.** Symptom: the Workflow step's gateway row had a null
  `card_uid`. Cause: `host.rs` built an unattributed
  `ServerWyrdGatewayCaller`. Fix site: `WorkflowHost::prepare` attributes the
  pinned Workflow Card with no Run (`8022c3076`). Other callers checked:
  `eval.rs` and the verification service already attribute.
- **V10 Python.** Symptom: `AttributeError: _native`. Cause: the test passed
  the client positionally into `Bifrost(table=…)`. Fix site: the test.
- **V11 classification.** Symptom: the judge pair set held A/A. Evidence: per
  span, `verification.*` spans carry the request pair, and `gateway.*` and
  `workflow.gateway.call` carry A/SYSTEM. Cause: the test counted
  `verification.judge` (the caller's operation) as a judge call. Fix site:
  the test selects gateway spans, matching V7/V10.

### Old-name disposition (AC-006)

`git grep -w` over the live tree, excluding `changes/`:

| Name | Remaining | Disposition |
|---|---|---|
| `caller_principal_id`, `requested_by_principal_id`, `subject_card_uid`, `audit_principal_id`, `created_by` | none | removed |
| `verifier_uid` | `wyrd.drift_baselines` key and its queries, fitter/direct/drift locals, `results.rs` field written as `verifier_card_uid`, MCP test fixture field | identity key/local names, not attribution |
| `principal_id` | `auth_refresh_tokens`, `platform.credentials`, `platform.principal_identities`, `auth_api_keys`, login-state and device-authorization bindings | owner/identity keys per spec |
| TokenExchange `AuditDetail.subject_principal_id`/`actor_principal_id` | audit detail | describes the minted token, retained |

### Limits and risks

- Manual queued runs are exercised for Drift only.
- Nested (multi-hop) delegation is not exercised beyond one actor.
- Local SDK Workflow calls carry no Workflow Card; the client Card header is
  checked against Card scope, so only hosted runs attribute the Workflow Card.
- V7/V10 assert direct span pairs; delegated span pairs are proven in V11 and
  delegated rows in V1/V5/V6/V9.
- The hosted V8 test had no separate RED; the same missing Workflow Card
  failed on the local path first.
