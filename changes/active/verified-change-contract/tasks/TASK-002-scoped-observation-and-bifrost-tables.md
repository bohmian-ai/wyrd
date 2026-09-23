---
id: TASK-002
kind: implementation
status: approved
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

## Implementation Record

Status: **IMPLEMENTED**. Scenarios 1-7 are committed and green.

### Scenario evidence

| Scenario | Where | Proof |
|---|---|---|
| 1 one Bifrost lifetime per `WyrdState` | `crates/shared/wyrd-client/src/state.rs`, `bifrost/lifecycle.rs` | `crates/shared/wyrd-client/src/observe/tests.rs` |
| 2 one invocation, Card-scoped sibling views | `crates/shared/wyrd-client/src/observe/{mod,run}.rs` | same |
| 3 Drift → tall fixed rows | `crates/shared/wyrd-client/src/observe/drift.rs`, `crates/vala/.../tables/drift` | same + `test:bifrost` unit tiers |
| 4 Eval canonical record | `crates/shared/wyrd-client/src/observe/eval.rs` | same |
| 5 generic `FixedSizeBinary(16)/(8)` hex | `crates/vala/vala-bifrost-redux/src/.../batch_builder.rs` | `-E 'test(=batch_builder::tests::builds_user_columns_plus_correlation_columns)'` |
| 6 `observe.record` describe-once + routing | `crates/shared/wyrd-client/src/observe/mod.rs` | `observe/tests.rs`, `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py`, `sdks/wyrd-sdk-ts/wyrd/tests/unit/observe.test.ts` |
| 7 real SDK journeys, all three languages | `sdks/wyrd-sdk-rust/tests/observe_run.rs`, `sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py`, `sdks/wyrd-sdk-ts/wyrd/tests/integration/observe-run.test.ts` | `test:bifrost:journey:{observe,python,typescript}` |

Each journey registers a Service graph of one Model, one Agent, and an
artifact-bearing Prompt; switches Model and Agent views inside one invocation;
emits a typed/mapping Drift input, an Eval context, and one generic
`vala.datasets` row; refuses a reserved table and an unknown alias; drains at
graceful shutdown; and reads every row back by the exact subject Card UID and
the one invocation id.

### Product changes the journeys forced

1. **Card-bound writer identity.** Registering a Service or Agent Card already
   upserts the service account bound to that exact `CardRef`
   (`wyrd-sql/src/queries/cards/auth_projection.rs`). A journey that minted its
   own principal got a bare fixture root as its card-ref scope, so scoping a
   view onto the registered Model or Agent was refused with
   `WYRD_VALA_403_BIFROST_CARD_SCOPE`, and minting under the Card's name
   collided with the projection's unique `(tenant, name)`.
   `WyrdTestServer::credential_registered_service` therefore credentials the
   projected principal instead of minting one, and is projected into Python
   (`crates/wyrd/wyrd-testing/src/python.rs`) and the TypeScript test addon
   (`sdks/wyrd-sdk-ts/native-testing/src/lib.rs`). `mint_machine_principal`
   lost its `seed_card` flag with its last `false` caller.
2. **Lazy built-in describe.** `BifrostCatalog::describe_table` materializes a
   lazy built-in before the row lookup. Ingest already materialized one on first
   use, but `start_bifrost` *describes* `vala.drift.observations` and
   `vala.eval.observations`, which 404'd for every tenant that had not written
   yet. The Rust journey pre-provisions nothing, which is what proves it.
3. **Loader diagnostics reach the caller.** `Cards::register_from_path` puts the
   loader's structured `diagnostics` in the error details; without them a caller
   sees `load failed with 3 diagnostic(s)` and no per-file code or remediation.
4. **`start_bifrost_with(client, table)`** replaced
   `start_bifrost_with_config(..., QueueConfig)`: `QueueConfig` is not
   re-exported from `wyrd-client`, so the old signature was uncallable from any
   SDK.

### Language-specific fixture notes

Python hydration eagerly loads every Model holder, so the Python fixture Model
carries an artifact and registers alone under the
`WYRD_REGISTRY_400_HEAVY_ARTIFACT_NOT_SOLE_SUBMISSION` rule, with a per-alias
interface standing in for its `Custom` loader. The Rust and TypeScript bundles
load no Model payload and need neither.

### Spec correction applied

`architecture/logic/run_api.md` showed `observe.record("app.events", ...)`.
That FQN is not legal: Gate's `resolve_fqn` accepts only known `vala.*`
namespaces and `vala.datasets` is the sole caller-owned one. The examples now
use `vala.datasets.*` and the rule is stated with them.
`architecture/bifrost-design.md` records the lazy built-in describe behavior.

### Defects the full journey lanes surfaced

Running every Bifrost lane to completion on a Linux host exposed eight defects
that had been masked behind each other. None originates in this task's write
set; each is fixed here rather than recorded as known, per AGENTS.md §12.

| Commit | Layer | Defect |
|---|---|---|
| `61e9b69a` | build config | Ten check tasks invoked a bare `python`. `mise` pins node, uv, rust, pnpm, cmake, and protoc but no Python, so `check:tenant-isolation` failed with `sh: 1: python: not found` and took `verify:bifrost` with it. They now run through `uv run --no-project python`. |
| `daaf2fad` | test harness | `AddressPlan::DistinctLoopbackAddresses` gave the pod at index N the address `127.0.0.(N+2)` at canonical ports, so every test process claimed `127.0.0.2:8080` for its first pod and five of six concurrent journeys lost the bind. The two middle octets now come from the process id. Invisible wherever `detect` falls back to `DistinctPortsOnLocalhost`. |
| `be3041fc` | **production** (`vala-sql`) | `freeze_publication_range` locked `vala.audit_chain_head` with an unbounded `FOR UPDATE`. `AuditPublisher::sweep` awaits every cycle in its `for_each_concurrent`, so one held head froze the whole sweep — and a sweep that never returns never re-lists tenants, leaving any tenant created after it began owing retained history forever. The wait is now bounded by a 3s `lock_timeout`, with `55P03` yielding the cycle to the next tick. Competitors still queue and reuse the frozen bound, which `NOWAIT` would have broken. |
| `81c446aa` | test harness | `reserve_loopback_addr` bound `:0`, read the address, and dropped the listener, handing the port straight back to the kernel's own ephemeral pool. Tests failed on ports they never touched. Reservations now come from `[ip_local_port_range_low / 2, low)`, walked from a pid-seeded offset, with `:0` retained as the non-Linux fallback. |
| `0df3bcf8` | test logic | `compact_sealed_batch` budgeted 32 *passes* and waited on the global Forge completion observer, which reports completions for every table — including the second tenant's table the published-governance journey registers itself. A burst spent the whole budget in 5.4s. The budget is now 120s of wall clock with a 100ms poll floor. |
| `1616bff5` | **production** (`vala-sql`) | The Forge fair claim gates on `t.ready_at <= statement_timestamp()`, but both enqueue INSERTs bound an absolute instant the planning scheduler stamped with a host-process `Utc::now()` — two different clocks. A process running ahead of PostgreSQL dates its task into the database's future, where the claim cannot see it for the duration of the skew, silently. Wyrd scales `wyrd-server` horizontally, so each replica carries its own offset and a drifted replica stalls the tasks it enqueues. The backoff paths already derived eligibility from `statement_timestamp()`; enqueue and the plain `retry` release did not (see the next row). `NewForgeTask::ready_at` is now an `Option` and the insert stamps `COALESCE($12, statement_timestamp())`. Latent wherever the test process and the database share a kernel clock, which is why it surfaced only against a containerized PostgreSQL whose VM clock had drifted. |
| _this commit_ | **production** (`vala-sql`) | `ForgeTasks::retry`, the non-consuming release used by bounded passes and cancelled claims, set `ready_at=$4` from the caller's clock: `Utc::now()` in one worker path and the Forge clock in the other, which a test fixture advances by a day to close a partition. Either way the released task sat outside the claim's clock domain. The parameter is removed and the statement stamps `statement_timestamp()`. |
| _this commit_ | test (`wyrd-client` SDK journey) | `oracle_query_status_cancel_is_tenant_scoped` asserts on `vala.audit_staging`, which the server's own `AuditPublisher` retires every 5 s. A run slower than one sweep lost the earliest data-tenant `running.get` row. The fixture now starts with `WyrdTestServerBuilder::without_audit_publication_for_test`, a `test-support`-only switch that keeps the publisher from starting. |

`cargo hakari generate --diff` also reported pre-existing drift
(`tracing-log`, `tracing-subscriber` missing from both platform sections),
which failed `check:workspace-hack` before any other gate task could run;
`5b6637b1` applies the generator's own output.

### Verification evidence

Every command in *Verification and Evidence* passed against `2f9e401d`, with
two command corrections:

- The queue regression's module is `batch_builder_tests`, so the exact
  command is `mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E
  'test(=batch_builder::batch_builder_tests::builds_user_columns_plus_correlation_columns)'`
  (1 passed). The listed `batch_builder::tests::` path selects no test.
- `mise run test:e2e` no longer exists; `a34dfa6e` folded its targets into the
  family lanes. Its client targets (`pg_auth_e2e_against_fixture`,
  `pg_discovery_against_fixture`) ran in `test:shared`; its surviving server
  targets ran directly under `scripts/postgres/with-test-postgres.sh` as
  `cargo nextest run --locked -p wyrd-server --test auth_e2e --test
  authz_check_e2e --test pg_authz_check_route --test-threads=1` (passed).
  `pg_bootstrap_key` no longer exists.

| Command | Result |
|---|---|
| `test:shared` | 686 passed |
| `test:wyrd-sdk` | passed |
| `verify:bifrost` | 9/9 lanes, including journeys `sdk`, `observe`, `oracle`, `python`, `typescript` |
| `test:bifrost:journey:{sdk,python,typescript}` | passed inside `verify:bifrost` |
| `py:test:unit`, `py:test:integration`, `py:typecheck` | passed |
| `ts:test:unit`, `ts:test:integration` (18), `ts:typecheck`, `ts:napi:check` | passed |
| `codegen:check`, `check:client-tier`, `check:pyo3-scope` | passed |
| `fmt`, `py:format`, `lints`, `py:lints`, `git diff --check` | clean |

The Forge clock-domain behavior previously described here as an environment
condition is the production defect fixed in `1616bff5` and `2f9e401d`.

## TASK-002-R1 remediation evidence

Remediation of `review/TASK-002-r1/TASK-002-R1-close-scoped-observation-gaps.md`
on the cumulative candidate `ceae3080..a96820fa`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-1 configured Rust startup | `WyrdState::start_bifrost_with_config` in `crates/shared/wyrd-client/src/state.rs` claims the lifecycle, then calls `Bifrost::connect_with_config`; `start_bifrost_with` delegates to it; `QueueConfig` is re-exported from `wyrd_client::bifrost`/`wyrd_client` and so `wyrd_sdk` (`7955b8e4`) | The Rust journey `sdks/wyrd-sdk-rust/tests/observe_run.rs::scoped_run_emits_drift_eval_and_generic_rows` compiles the locked call `start_bifrost_with_config(&client, None, QueueConfig::default())` and passes in `test:wyrd-sdk` / `verify:bifrost` | PASS |
| FIND-2 exact fixed schema | `require_projection` in `observe/mod.rs` compares the ordered `(name, type, nullable)` sequence; `drift.rs`/`eval.rs` declare nullability (`cd922894`) | `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=observe::tests::incompatible_fixed_table_fails_startup)'` covers reordered, extra, wrong-type and wrong-nullability; the canonical schemas start in every journey | PASS |
| FIND-3 Python keys | `require_string_keys` in `sdks/wyrd-sdk-python/src/observe/mod.rs` runs after reduction and before `json.dumps` (`b5619716`) | `test_drift_refuses_a_non_string_mapping_key[1,1.5,True,None]` and `test_eval_checks_keys_after_reduction` in `tests/unit/state/test_observe_surface.py` via `mise run py:test:unit` | PASS |
| FIND-4 TypeScript strict JSON | `strictJson` in `sdks/wyrd-sdk-ts/wyrd/src/index.ts`, used by drift, eval, record and media (`192c695f`) | In `tests/unit/observe.test.ts`, "refuses every value JSON.stringify would drop or coerce before native" and "calls native exactly once for valid nested JSON" pass via `mise run ts:test:unit` | PASS |
| FIND-5 active spans | Python `active_span_ids` and TS `activeSpanIds` are read only when both explicit IDs are absent (`92d5efc3`) | Python `test_active_span_reaches_the_writer` and `test_explicit_span_without_trace_is_refused_inside_an_active_span`; TS "takes both ids from the active span only when neither is explicit"; the Python journey reads back the active, explicit and absent IDs (`py:test:integration`) | PASS |
| FIND-6 single describe | `describe_gate` in `bifrost/facade.rs`: check the cache, lock, recheck, describe (`9c9a1d17`) | `-E 'test(=observe::tests::concurrent_first_records_describe_each_table_once)'`: one describe per FQN, two producers; fails with the gate removed (mutation check) | PASS |
| FIND-7 terminal shutdown | `BifrostLifecycle::shutdown` closes from NotStarted/Starting and after a drain; `StartClaim::complete` publishes only from Starting (`8d9d9381`) | `-E 'test(=observe::tests::shutdown_without_startup_closes_permanently)'` and `-E 'test(=observe::tests::shutdown_during_start_stays_closed)'` | PASS |
| FIND-8 ambiguous shutdown retry | Unchanged producer retry path, proven at the state level (`8d9d9381`) | `-E 'test(=observe::tests::ambiguous_shutdown_retries_the_same_batch_on_the_same_state)'`: same batch id attempted and received, then CLOSED on write and restart | PASS |
| FIND-9 audit lock timeout | `freeze_publication_range` in `crates/vala/vala-sql/src/queries/audit_staging.rs` propagates 55P03 as `SqlError` (`98ed762e`) | `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p vala-sql --test pg_audit_staging -E 'test(=pg_tests::audit_staging::held_chain_head_times_out_explicitly_and_retries_unchanged)'` | PASS |
| FIND-10 module rustdoc | `//!` docs on the drift/eval/verification table modules and documented `VerificationContract` fields (`624b5a67`) | `mise run lints` (workspace clippy `-D warnings`, `--all-features`) clean, with no suppressions added | PASS |
| FIND-11 imports/signatures | Test-only imports moved into the tests module; `eval.rs` OpenTelemetry traits at module level; `lifecycle.rs` `Debug` uses imported `fmt` types (`624b5a67`) | `mise run lints`, `mise run fmt` clean | PASS |
| FIND-12 real negative describe | Unknown-table refusals in the Rust, Python and TS journeys; `denied_describe_is_audited_before_admission` in `crates/shared/wyrd-client/tests/pg_bifrost_e2e.rs` (`3ee2bf88`) | Three journeys assert `WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND` (`verify:bifrost`, `py:test:integration`, `ts:test:integration`); the e2e asserts `WYRD_PERMISSION_403_DENIED_RBAC`, one `denied` `vala.bifrost.describe` audit row, no cached table and zero producers: `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-client --test pg_bifrost_e2e -P journey --run-ignored=all -E 'test(=denied_describe_is_audited_before_admission)'"` (1 passed; also inside `verify:bifrost` journey `sdk`) | PASS |

Closure (every command passed at `a96820fa`):

| Command | Result |
|---|---|
| `verify:bifrost` | 9/9 lanes passed, including the journeys `sdk`, `observe`, `python` and `typescript` |
| `test:shared` | 689 passed |
| `test:wyrd-sdk` | passed |
| `py:test:unit`, `py:test:integration`, `py:typecheck` | passed |
| `ts:test:unit`, `ts:test:integration`, `ts:typecheck`, `ts:napi:check` | passed |
| `codegen:check`, `check:client-tier`, `check:pyo3-scope` | passed |
| `fmt`, `py:format`, `lints`, `py:lints`, `git diff --check` | clean |

The five named `observe::tests::*` unit tests above also ran together through one `-E` expression (5 passed).

Queue regression: `mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(=batch_builder::batch_builder_tests::builds_user_columns_plus_correlation_columns)'`, 1 passed.

Non-goals: no new queue, transport, schema cache, producer cache, config type,
or lifecycle vocabulary. `QueueConfig` is the existing `wyrd_queue` type,
re-exported. No flush, verdict wait, run registry, atomic multi-row API,
per-FQN single-flight framework, authz model, audit sink, lease or owner token
was added. The describe gate is the owner-wide miss gate; its ceiling is marked
`ponytail:`. No generated stub or schema was hand-edited. `codegen:check` is
clean.

Material limits:
- The TS SDK requires `@opentelemetry/api` lazily and declares no package
  dependency. When the package is absent, IDs fall back to explicit-or-null.
- The describe gate serializes cache-miss describes across all FQNs within one
  `Bifrost`; cache hits are unaffected.

## TASK-002-R2 remediation evidence

Remediation of `review/TASK-002-r2/TASK-002-R2-close-boundary-and-journey-gaps.md`
on the cumulative candidate `a000c201..4fc251ce`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-4 symbol keys | `strictJson` in `sdks/wyrd-sdk-ts/wyrd/src/index.ts` refuses any own symbol key of a plain object through the existing `invalidObservationInput` path before traversal (`2fb00f19`) | `tests/unit/observe.test.ts` refused-input table gains root and nested own-symbol-key rows asserting `WYRD_SPEC_400_VALIDATION` and zero fake-native calls: `pnpm exec vitest run tests/unit/observe.test.ts` (8 passed); `ts:test:unit` | PASS |
| FIND-10 rustdoc | Docs, with `# Errors`/`# Panics` where fallible or panicking, on every undocumented added/changed item: `DomainTable` associated items in the drift/eval/verification table modules, `observe/tests.rs` `Features`/`score`, `batch_builder` `build_column` and tests, `BifrostNamespace`, the built-in fingerprint test, vala-eval/CLI test helpers, `message_details`, and `wyrd-testing` server items (`50f38108`); every item added in this remediation is documented | A source audit of the complete `c8bb490a..` range over added/changed Rust declarations; `lints` clean; no suppression or new check | PASS |
| FIND-13 Eval journeys | Rust: explicit session, media, and trace/span through `EvalObservationOptions::from_parts` (`a87f0e8b`). Python: active and explicit cases retained; the explicit case adds `session_id` and `MediaRef` (`cd0b8945`). TS: a real `BasicTracerProvider` span active through the shared `StorageContextManager` (moved to `tests/support/otel-context.ts`) crosses N-API with session and media (`b9ea02a4`) | Each journey reads back exact session, media JSON, and non-null trace/span hex, then proves span-without-trace and a `hologram` media kind both fail with `WYRD_SPEC_400_VALIDATION`, with exactly the expected Eval rows persisted: Rust `observe_run::scoped_run_emits_drift_eval_and_generic_rows`; Python `tests/integration/state/test_observe_journey.py::test_scoped_run_emits_drift_eval_and_generic_rows`; TS `observe-run.test.ts` | PASS |
| FIND-14 startup/cache/fingerprint | `WyrdTestServer::{table_describe_count, fail_table_describe, restore_table_describe}` in `crates/wyrd/wyrd-testing/src/server.rs`, projected to Python (`python.rs`, `audit_publication` flag) and TS (`native-testing`, `startTestServer(auditPublication?)`). Each journey starts with audit publication off and loops over both fixed tables: fault → startup refused with `WYRD_VALA_500_AUDIT_UNAVAILABLE` → restore → state starts. It then writes one dataset twice with describe count `1`, and the fixed-table counts stay unchanged across emits. `assert_stale_writer_is_fenced` in `pg_bifrost_e2e.rs` sends a stale-schema flush and register, both refused with `WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH`, and no stale row lands (`906a621f`) | The three journeys above, plus `pg_bifrost_e2e -E 'test(=pg_tests::unified_client_registers_writes_swaps_and_reads_both_tables)'` (passed) | PASS |

Focused commands (all passed):

- `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test observe_run -P journey --run-ignored=all -E 'test(=scoped_run_emits_drift_eval_and_generic_rows)'"`
- The same wrapper with `-p wyrd-client --test pg_bifrost_e2e -P journey --run-ignored=all -E 'test(=denied_describe_is_audited_before_admission) | test(=pg_tests::unified_client_registers_writes_swaps_and_reads_both_tables)'` (2 passed)
- The same wrapper with `mise run db:migrate:inner` and `uv run python -m pytest -q -m integration tests/integration/state/test_observe_journey.py::test_scoped_run_emits_drift_eval_and_generic_rows` in `sdks/wyrd-sdk-python`
- `mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(=batch_builder::batch_builder_tests::builds_user_columns_plus_correlation_columns) | test(=batch_builder::batch_builder_tests::fixed_size_binary_rejects_malformed_and_wrong_width_hex) | test(=batch_builder::batch_builder_tests::fixed_size_binary_round_trips_hex_trace_and_span_ids)'` (3 passed)
- `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E` over the five `observe::tests::` lifecycle and concurrent-describe tests named in R1 (5 passed)
- `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p vala-sql --test pg_audit_staging -E 'test(=pg_tests::audit_staging::held_chain_head_times_out_explicitly_and_retries_unchanged)'` (1 passed)

Closure: `test:shared` (689 passed), `test:wyrd-sdk`, `py:test:unit`,
`py:test:integration`, `py:typecheck`, `ts:test:unit`, `ts:test:integration`
(18 passed), `ts:typecheck`, `ts:napi:check`, `codegen:check`,
`check:client-tier`, `check:pyo3-scope`, `fmt`, `py:format`, `lints`,
`py:lints`, and `git diff --check` all passed. `verify:bifrost` passed 9/9
lanes at `4fc251ce`.

Non-goals excluded: there is no new serializer, dependency, config type, queue,
cache, producer pool, transport, schema authority, lifecycle state, or
authorization model. The owner-level concurrent-miss test is not copied into any
language. No verdict wait, flush, multi-row API, run registry, or retention was
added, and there is no documentation tool or suppression. The test-only
`StorageContextManager` moved to a shared test module rather than being
duplicated. Generated stubs and `.d.ts` files were regenerated, not
hand-edited.

Material limits:
- The describe fault and describe count are test-harness probes: a Postgres
  trigger on `vala.audit_staging` and a count of staged
  allowed-`vala.bifrost.describe` rows. They are accurate only while audit
  publication is off, which is why each journey disables it.
- The first `verify:bifrost` run failed one test outside TASK-002:
  `wyrd-server::pg_router_smoke oracle_epoch_cutoff_removes_readiness_and_retirement_joins_loss_owner`
  (`blocked renewal suppressed cutoff; admits=false, waiters=1`, at the 15 s
  `FORGE_READINESS_CEILING`). It passed three times in isolation (about 29 s
  each) and passed in the full rerun. Root cause: the test's mid-renewal
  `collapse_lease_for_test()` could not take effect, because the supervisor
  sleeps on a cutoff captured before the stalled renewal, so the test raced
  the real cutoff (about 15 s away) against a 15 s ceiling. Fixed test-side:
  the wait is now bounded by the Postgres-reported lease expiry minus the
  10 s cutoff lead plus 2 s scheduling slack, and
  `forge::reader_expiry_ordering::reader_and_expiration_claim_have_one_table_local_winner`
  now waits for the collapsed epoch to close admission rather than assuming
  it did. Both pass in their focused `cargo nextest` runs under
  `scripts/postgres/with-test-postgres.sh`. Not covered: renewal and cutoff
  becoming ready in the same `select!` poll.
- The R1 limits stand: the lazy TS `@opentelemetry/api` require, and the
  owner-wide describe gate.

## TASK-002-R3 remediation evidence

Remediation of `review/TASK-002-r3/TASK-002-R3-close-contract-and-standards-gaps.md`
on the cumulative candidate `04f73570..d6231892`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-4 own-data omission | `strictJson` in `sdks/wyrd-sdk-ts/wyrd/src/index.ts` walks `Reflect.ownKeys` before traversal: a plain object refuses symbol and non-enumerable keys; an array refuses every own key other than `length` and canonical in-range indices (`isArrayIndex`). The serializer is unchanged otherwise (`60f34518`) | `tests/unit/observe.test.ts` refused-input table adds root and nested non-enumerable, array non-index, and array symbol-key rows. Each row goes through Drift, Eval, and record and asserts `WYRD_SPEC_400_VALIDATION` with zero fake-native calls. The valid nested JSON test is unchanged: `pnpm exec vitest run tests/unit/observe.test.ts` (8 passed; failed before rebuild); `ts:test:unit` (20 passed) | PASS |
| FIND-11 imports | `wyrd-testing/src/server.rs`: `Range`, `OnceLock`, `AtomicU32` join the top std group, and `service_account_by_card_ref` moves from the late block to the top `wyrd_sql` import. `reservation_band`, `BAND`, `CURSOR`, `reserve_loopback_addr`, and `bound_loopback_addr` use bare types. `pg_router_smoke.rs`: a `test-support`-gated `use std::time::Duration` in the std group, with both cutoff constants typed bare (`9d0cbf77`) | `fmt`, `lints`, `check:clippy-allow-audit` passed; the cutoff test passed inside `verify:bifrost` | PASS |
| FIND-16 closed shapes | `EvalMediaRef` and `EvalOptions` are exported readonly object type aliases with identical fields and docs (`60f34518`) | `ts:typecheck`, `ts:test:unit`, `ts:test:integration` (18 passed) | PASS |
| FIND-17 evidence code | The R2 FIND-4 row now names `WYRD_SPEC_400_VALIDATION`, matching `invalidObservationInput` and the unit assertion (`d6231892`) | Inspection; `git diff --check` | PASS |

Closure: `verify:bifrost` passed 9/9 lanes at `d6231892`, including
`denied_describe_is_audited_before_admission`,
`held_chain_head_times_out_explicitly_and_retries_unchanged`,
`oracle_epoch_cutoff_removes_readiness_and_retirement_joins_loss_owner`, and
`reader_and_expiration_claim_have_one_table_local_winner`. Also passed:
the three `wyrd-queue` fixed-size-binary/correlation `batch_builder_tests`
(3 passed), `wyrd-client --lib -E 'test(/^observe::tests::/)'` (23 passed,
covering the lifecycle and concurrent-describe tests), `ts:test:unit`,
`ts:typecheck`, `ts:test:integration`, `fmt`, `lints`,
`check:clippy-allow-audit`, and `git diff --check`.

Non-goals excluded: no new serializer, dependency, error code, queue, cache,
pool, transport, lifecycle state, or style check was added; there is no Rust
duplicate of the TypeScript check; unrelated pre-existing qualified
expressions were not touched; generated declarations were not hand-edited.

## TASK-002-R4 remediation evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-002-4`: getter-backed fields and array elements are read once and native receives the first accepted value; open media descriptors fail with `WYRD_SPEC_400_VALIDATION` before native | `sdks/wyrd-sdk-ts/wyrd/src/index.ts` `strictJson` builds and stringifies one snapshot; `mediaJson` checks descriptor own keys against `MEDIA_KEYS` (`935cc6324`) | `observe.test.ts` accessor and media rows (red before fix, 10/10 after); `ts:test:unit` 22 passed; `ts:typecheck`; `ts:test:integration` 18 passed | PASS |
| Lane blocker: journey port collision (`Address already in use`) | `crates/wyrd/wyrd-testing/src/server.rs` `claim_port` fences band ports with a per-port file lock held for process lifetime (`4642bab94`) | focused `capacity::memory_refusal_preserves_oracle_health_and_next_query` passed; `fmt`, `lints`, `check:clippy-allow-audit`, `check:unwrap-audit`, `git diff --check` | PASS |

Limit: `verify:bifrost` ran once before the port fix (8/9 lanes; the only failure was the port collision above). The full rerun after `4642bab94` was stopped by the user and was not completed.

Non-goals held: no new serializer, dependency, public type, error code, or Rust duplicate of TypeScript validation.
