---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-verified-change-contract
spec_revision: 32
requirements: [REQ-075, REQ-076, REQ-118, REQ-121, REQ-122, REQ-123, REQ-124, REQ-125, REQ-126, REQ-127, REQ-128, REQ-129, REQ-132, REQ-133, REQ-145, INV-007, INV-010, INV-012, AC-017, AC-020, AC-024, AC-025, AC-026]
depends_on: [TASK-001]
---

## Outcome and Value

Rust, Python, and TypeScript users start one state-owned Bifrost writer, open
one invocation, select immutable Card scopes, and enqueue Drift, Eval, or
generic records through the existing bounded queue. The five fixed verification
tables have the approved physical schemas and identity split; no observation
call switches an active table, invents a schema, or waits for a verdict.

## Owners, Scope, Consumers, and Prohibited Changes

`wyrd-client::WyrdState` owns lifecycle and scoped authoring; its existing
`Bifrost` facade owns table description/cache, one `WriterPool`, transport,
flush, and shutdown. `wyrd-queue` alone performs generic schema-driven
JSON-to-Arrow conversion. Drift/Eval boundaries own their canonical-record and
fixed-row projections. Vala's built-in table catalog owns physical layouts;
Gate/Scribe own authenticated admission, scope resolution, stamps, and ACKs.

Do not expose `WriterPool`, add another Bifrost client/queue/schema system,
dispatch by Verifier kind in shared plumbing, infer a table from Card identity,
or put `card_ref`/`run_id` in user rows. Do not add per-observation flush or
schema IO.

## Approach

1. Register the exact five schemas, daily layouts, Bloom columns, sensitivity,
   and retired-table removals from `table_schema.md`.
2. Extend the existing Bifrost facade with owner-mediated explicit-table
   insertion and a connected-writer schema cache; reuse `WriterPool` producers.
3. Add invocation/scoped views and canonical Drift/Eval projections in shared
   Rust, then thin Python/TypeScript authoring boundaries.
4. Add generic fixed-size-binary hex conversion to the queue builder.
5. Prove lifecycle, routing, correlation, and all three first-class surfaces
   against real server/Postgres journeys.

## Ordered Implementation Scenarios

### Scenario 1 — One Bifrost lifetime per state

**Behavior.** Startup passes through existing options, describes both fixed
tables before success, refuses a second start, drains every producer, permits
same-handle shutdown retry after ambiguity, and permanently closes after a
successful shutdown.

**RED.** Add lifecycle tests covering missing/incompatible fixed tables,
repeated start, failed shutdown retry, and closed-state refusal. Current
`WyrdState` owns no such lifecycle.

**GREEN.** Compose the existing Bifrost facade into shared state and preserve
one runtime-owned writer across state clones.

**REFACTOR.** Consolidate state transitions on the existing owning structs;
do not introduce a second lifecycle abstraction.

### Scenario 2 — One invocation supports immutable Card scopes

**Behavior.** `run()` mints one UUIDv7 invocation ID and starts at the root
Service; alias selection returns immutable sibling views sharing that ID but
carrying their exact UID-bearing CardRef. Unknown/out-of-graph aliases fail
without network IO.

**RED.** Add root, Model, Agent, sibling-isolation, and invalid-alias cases.
They fail because the locked API does not exist.

**GREEN.** Reuse the hydrated state index for lookup and return small scoped
views over the shared invocation/state.

**REFACTOR.** Keep scope values immutable and remove any active-Card mutation.

### Scenario 3 — Drift inputs become canonical tall rows before enqueue

**Behavior.** Rust `Serialize`, Python mapping/dataclass/Pydantic JSON, and a
TypeScript plain object converge on the existing validated feature map and one
tall row per feature. Null/nested/nonfinite/unsafe numeric/invalid-name inputs
fail before admission; numeric category strings match baseline fitting.

**RED.** Add boundary tests for all accepted families and refusals, including
direct `model_dump_json()` handoff and exact integer-to-Float64 limits.

**GREEN.** Deserialize to existing `FeatureName`/`FeatureValue`, construct the
canonical record, project fixed rows, and insert with correlation.

**REFACTOR.** Share Rust-native validation/projection; keep only unavoidable
foreign-runtime serialization at Python/TypeScript edges.

### Scenario 4 — Eval inputs preserve context, trace, and media identity

**Behavior.** Each SDK builds the existing Eval record from context and
options, generates record/time identity, prefers explicit trace IDs then a
valid active span, rejects span-without-trace and malformed media, and projects
the fixed row without `eval_ref` or duplicate `run_id`.

**RED.** Add per-language accepted/invalid cases and active-span precedence.
They fail against the old canonical record and absent wrapper.

**GREEN.** Extend only the existing Eval media item with approved binding
identity/kind and project canonical JSON descriptors.

**REFACTOR.** Keep Prompt media and Eval media publicly distinct while sharing
Rust-native validated primitives where already available.

### Scenario 5 — Fixed binary IDs are generic queue types

**Behavior.** Described `FixedSizeBinary(16)` and `(8)` columns accept exact
lowercase hex, reject malformed/wrong-width input, round-trip trace/span bytes,
and remain type-compatible with trace tables.

**RED.** Extend the schema-driven batch-builder test; it currently lacks this
datatype support.

**GREEN.** Add the minimum Arrow builder branch in `wyrd-queue`, independent
of Eval.

**REFACTOR.** Keep width/error handling in the generic datatype conversion,
not in observation code.

### Scenario 6 — Dynamic records describe once and route explicitly

**Behavior.** First use describes an authorized registered table, concurrent
first uses converge, later writes reuse its schema/producer, and two tables and
Card scopes retain their own correlation without active-table mutation.
Unknown/unauthorized/reserved tables fail before admission; stale schemas are
refused by the fingerprint fence.

**RED.** Add cache-convergence, two-table concurrency, scope isolation, and
reserved-table cases around public `observe.record`.

**GREEN.** Reuse Bifrost table description plus `WriterPool::insert` through a
safe facade operation and first-schema-wins cache.

**REFACTOR.** Delete redundant table/producer caching; `WriterPool` remains the
only producer pool.

### Scenario 7 — Real SDK journeys cross queue, IPC, Gate, and Scribe

**Behavior.** Each SDK switches Model/Agent scopes in one invocation, emits a
typed and mapping/plain input, writes two generic tables, drains explicitly,
and queries rows with the exact subject Card UID and one invocation ID. Normal
calls perform no per-record describe/flush and share the bounded budget.

**RED.** Add gated real-SDK journeys against `WyrdTestServer` and repository
Postgres. They fail until Scenarios 1–6 are integrated.

**GREEN.** Wire only missing public projections and generated typing.

**REFACTOR.** Keep one shared Rust implementation; language packages retain
only runtime-earned wrappers.

## Acceptance Criteria

- Exact schemas/order/nullability/partitions/Blooms match `table_schema.md`.
- All three public run APIs match `run_api.md` and preserve the writer/subject
  identity split.
- Graceful shutdown is the durability barrier; admission is never described
  as a Scribe ACK.
- Shared client and queue code remain Verifier-kind agnostic.

## Expected Write Set and Consumer Closure

Likely owners: `crates/shared/wyrd-client/src/{state,bifrost}`, `wyrd-queue`'s
batch builder, canonical observation records in `wyrd-spec`, Vala built-in
table catalog/schema tests, Python/TypeScript SDK state/Bifrost wrappers and
generated types, Rust SDK journeys, Gate/Scribe integration tests, and
`architecture/bifrost-design.md`.

## Verification and Evidence

Run each new focused test by exact name once created. A confirmed existing
queue regression is:

```bash
mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(=batch_builder::tests::builds_user_columns_plus_correlation_columns)'
mise run test:shared
mise run test:wyrd-sdk
mise run verify:bifrost
mise run test:bifrost:journey:sdk
mise run test:bifrost:journey:python
mise run test:bifrost:journey:typescript
mise run test:e2e
mise run py:test:unit
mise run py:test:integration
mise run py:typecheck
mise run ts:test:unit
mise run ts:test:integration
mise run ts:typecheck
mise run ts:napi:check
mise run codegen:check
mise run check:client-tier
mise run check:pyo3-scope
mise run fmt
mise run py:format
mise run lints
mise run py:lints
git diff --check
```

## Material Stop Conditions

Stop if correctness requires a second client/queue/transport, a new durable
observation type, changed managed-column identity, a per-observation ACK, a
schema supplied by callers, or weakened reserved-table/scope authorization.

## Authority Links

- `changes/active/verified-change-contract/spec.md`
- `changes/active/verified-change-contract/architecture/logic/run_api.md`
- `changes/active/verified-change-contract/architecture/logic/table_schema.md`
- `architecture/bifrost-design.md`
- `architecture/references/languages/testing-workflows.md`
- `AGENTS.md`
