# TASK-002 Wave 2 Findings Validation

## Immutable subject

- Base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Candidate: `fbfc2591a985b288935180098f892aecdf3b8b49`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Task: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md`
- Locked logic authorities: `changes/active/verified-change-contract/architecture/logic/run_api.md` and `table_schema.md`

The complete base-to-candidate diff, every Wave 1 report, the changed owners,
their callers, and the applicable repository authorities were inspected. The
candidate needs bounded remediation. No retained correction changes a product,
public-contract, architecture, security, compatibility, cross-service,
concurrency-ownership, resource-ownership, or persistent-data decision already
fixed by revision 32; `SPEC_REVISION_REQUIRED` does not apply.

## Wave 1 proposal validation

| Wave 1 proposal | Disposition | Independent validation and smallest safe correction |
|---|---|---|
| `TASKREV-001` | **CONFIRMED** | `WyrdState` exposes only `start_bifrost_with(&client, table)` and hard-codes `QueueConfig::default()` (`state.rs:440-466`), while the locked Rust example requires `start_bifrost_with_config(&client, Some(table), queue_config)`. Restore that exact method over existing `Bifrost::connect_with_config` and publicly re-export the existing queue config needed to call it; do not add another options type. Retained as `FIND-TASK-002-1`. |
| `TASKREV-002` | **CONFIRMED** | Both fixed-table preflights call `require_projection`, whose full body uses name lookup and datatype comparison only (`observe/mod.rs:305-331`). It accepts extra, reordered, and wrong-nullability fields despite the exact fixed schemas in `table_schema.md`. Compare the complete described user-field sequence by name, order, datatype, and nullability. Retained as `FIND-TASK-002-2`. |
| `TASKREV-003` | **CONFIRMED** | `json_text` passes mappings and `dataclasses.asdict` output directly to `json.dumps`; Python's native encoder accepts and stringifies non-string keys before Rust can detect them (`python/src/observe/mod.rs:37-75`). Validate the top-level keys of those two paths as `str` before the existing strict dump. Retained as `FIND-TASK-002-3`. |
| `TASKREV-004` | **CONFIRMED** | All three TypeScript observation calls and media conversion use raw `JSON.stringify` (`index.ts:1068-1146`), which omits `undefined`/functions/symbols and coerces non-finite numbers. Use one boundary-owned strict JSON serializer for Drift, Eval, generic rows, and media; reject unsupported values, non-finite numbers, and unsafe integers before the native call. Retained as `FIND-TASK-002-4`. |
| `TASKREV-005` | **CONFIRMED** | The only implicit lookup is Rust `tracing::Span::current()` (`observe/eval.rs:192-214`). The complete Python and N-API/TypeScript call bodies pass omitted IDs through without reading their runtime's active OpenTelemetry context. Read the owning runtime context only when both explicit IDs are absent and pass the resulting IDs through the existing options path. Retained as `FIND-TASK-002-5`. |
| `TASKREV-006` | **REVISED** | The check/describe/insert body at `bifrost/facade.rs:304-315` permits duplicate concurrent describes and its documentation admits that behavior. Per-FQN machinery is unnecessary: the smallest sufficient correction is one async cache-miss gate on the existing `Bifrost` owner, followed by a second cache check, so only one miss describes while the existing map and `WriterPool` remain authoritative. Retained as `FIND-TASK-002-6`. |
| `RS-001` | **REVISED** | PostgreSQL `55P03` aborts the transaction. Returning `Ok(None)` at `audit_staging.rs:271` does not actually produce a successful idle cycle because the sole production caller then commits and receives an aborted-transaction error (`audit/publication.rs:359-365`). The real defect is the SQL API reporting success from an unusable caller-owned transaction, not silent production `Idle`. Propagate the timeout through the existing staging-error path. Retained as `FIND-TASK-002-9`. |
| `RS-002` | **CONFIRMED** | `shutdown` maps `NotStarted` and `Starting` to success without changing phase, while `StartClaim::complete` unconditionally publishes `Started` and `Drop` unconditionally restores `NotStarted` (`observe/lifecycle.rs:91-103,134-141,171-199`). Fence all transitions in this owner so success is terminal and an in-flight claim cannot reopen it. Retained as `FIND-TASK-002-7`. |
| `RS-003` | **CONFIRMED** | Duplicate of `TASKREV-005`; retained under `FIND-TASK-002-5`. |
| `RS-004` | **REJECTED** | The path is reachable, but the proposed atomic multi-row admission contradicts the approved interface. `run_api.md` explicitly fixes “one row at a time into the existing bounded producer” and says not to claim one multi-feature observation is atomic queue admission. Replacing it with a batch-atomic producer operation would broaden queue semantics beyond TASK-002. No correction. |
| `RS-005` | **CONFIRMED** | The six new table module files begin with imports or bare module declarations and have no module rustdoc; the new `eval` and `verification` module items are likewise undocumented. `architecture/agent-rules.md` makes documentation of every new Rust module/item a hard gate. Add only the missing module/item rustdoc; no new lint or abstraction. Retained as `FIND-TASK-002-10`. |
| `RS-006` | **CONFIRMED** | Full bodies confirm function-scoped imports in `active_span_identity` and `verification_contracts`, plus fully qualified signature types in `BifrostLifecycle`'s `Debug` impl and Python `invalid_argument`. These violate the explicit top-level-import/bare-signature rule. Move the existing imports; behavior is unchanged. Retained as `FIND-TASK-002-11`. |
| `DC-1` | **CONFIRMED** | Duplicate of `RS-002`; retained under `FIND-TASK-002-7`. |
| `DC-2` | **CONFIRMED** | Duplicate of `TASKREV-004`; retained under `FIND-TASK-002-4`. |
| `DC-3` | **CONFIRMED** | Duplicate of `TASKREV-003`; retained under `FIND-TASK-002-3`. |
| `DC-4` | **CONFIRMED** | Duplicate of `TASKREV-005`; retained under `FIND-TASK-002-5`. |
| `SEC-AUD-001` | **REVISED** | Duplicate of `RS-001`. The timeout is already surfaced when the caller's commit fails, so the claimed silent `Idle` consequence is false; the transaction contract is still broken at the SQL function boundary. Retained under `FIND-TASK-002-9`. |
| `SEC-TEN-001` | **REVISED** | The implementation paths already derive tenant/publisher identity from credentials, enforce signed Card scope in Scribe, and have real server tests for those shared owners. TASK-002 does not own AC-030's full authorization/audit matrix, and no object-specific table-read authorization mechanism exists to test. The proposal does identify one explicit TASK-002 gap: AC-025 requires real-boundary evidence for unknown/unauthorized dynamic-table refusal, while the new SDK journeys exercise only the local reserved-name guard and the unknown case is mock-only. Retained only at that boundary as `FIND-TASK-002-12`. |
| `CONC-001` | **CONFIRMED** | Duplicate of `RS-002`; retained under `FIND-TASK-002-7`. |
| `CONC-002` | **REVISED** | Duplicate of `TASKREV-006`; the required convergence needs only one owner-local async miss gate, not a speculative per-key single-flight subsystem. Retained under `FIND-TASK-002-6`. |
| `CONC-003` | **CONFIRMED** | `WyrdState` tests cover successful started shutdown and a never-started no-op only (`observe/tests.rs:413-443`). Queue tests prove lower-level retained batches, but no caller drives an ambiguous failure through `WyrdState::shutdown` and retries the same lifecycle, despite TASK-002 Scenario 1 explicitly requiring that proof. Reuse the existing mock sink and state seam. Retained as `FIND-TASK-002-8`. |

The data/durability reviewer proposed no findings. Its empty proposal set was
validated against the five canonical table definitions, managed-column
composition, fixed-width ID conversion, and explicit-table write path; no
additional TASK-002 finding is introduced from that report.

## Deduplicated retained finding ledger

### `FIND-TASK-002-1`

- **Wave 1 source IDs:** `TASKREV-001`
- **Status:** `CONFIRMED`
- **Classification:** `VIOLATION`
- **Violated obligation:** REQ-123 and TASK-002's exact public-API acceptance require the configured Rust startup form locked in `run_api.md`.
- **Exact location:** `crates/shared/wyrd-client/src/state.rs:440-466`; authority `changes/active/verified-change-contract/architecture/logic/run_api.md:155-162`.
- **Evidence:** `start_bifrost_with` accepts only client and optional table and supplies `QueueConfig::default()` itself. There is no `start_bifrost_with_config`, and `wyrd-sdk-rust` merely re-exports `wyrd-client`, so the approved call cannot compile.
- **Observable consequence:** Rust callers cannot use the existing configured queue constructor through state-owned startup, and the locked example is false.
- **Decision-complete correction:** On `WyrdState`, expose `start_bifrost_with_config(&WyrdClient, Option<TableConfig>, QueueConfig)` and route it directly through the existing `StartClaim` and `Bifrost::connect_with_config`. Re-export the existing `wyrd_queue::QueueConfig` through the shared client/Rust SDK surface. Keep `start_bifrost_with` only if it is the default-config convenience over that exact method; add no new config type or constructor vocabulary.
- **Focused closure proof:** A Rust SDK compile/runtime test imports `QueueConfig` from `wyrd_sdk`, calls the exact approved configured form, and proves the supplied non-default queue setting reaches the existing constructor while fixed-table preflight still runs.

### `FIND-TASK-002-2`

- **Wave 1 source IDs:** `TASKREV-002`
- **Status:** `CONFIRMED`
- **Classification:** `INCORRECT`
- **Violated obligation:** REQ-127, AC-025, TASK-002 Scenario 1, and `table_schema.md` require incompatible fixed tables to fail startup and fix the exact authored order, type, and nullability.
- **Exact location:** `crates/shared/wyrd-client/src/observe/mod.rs:305-331`, called only by `observe/drift.rs:52-54` and `observe/eval.rs:93-95`, which are called by `StartClaim::complete` at `observe/lifecycle.rs:171-175`.
- **Evidence:** `require_projection` uses `field_with_name` and compares only `DataType`; it never compares field count, ordinal position, or nullability.
- **Observable consequence:** startup succeeds against a stale fixed-table schema with reordered, extra, or wrong-nullability authored fields and defers or hides the incompatibility.
- **Decision-complete correction:** Keep validation in the existing shared `require_projection` helper, but make each projection declaration carry expected nullability and compare the described user fields as one exact ordered sequence: count, name, datatype, and nullability. Do not compare or author managed columns and do not create another schema owner.
- **Focused closure proof:** Focused startup tests reject one reordered field, one wrong-nullability field, and one extra authored field for the fixed tables, while both canonical schemas still start successfully.

### `FIND-TASK-002-3`

- **Wave 1 source IDs:** `TASKREV-003`, `DC-3`
- **Status:** `CONFIRMED`
- **Classification:** `INCORRECT`
- **Violated obligation:** REQ-124 and the locked Python API require mappings and dataclass reductions to have string keys before strict JSON serialization.
- **Exact location:** `sdks/wyrd-sdk-python/src/observe/mod.rs:37-75`; callers are `PyObserveHandle::{drift,eval,record}` and `json_array_text` in the same module.
- **Evidence:** Mapping/dataclass output reaches `json.dumps(allow_nan=False)` with no key inspection; Python converts integer, float, boolean, and `None` keys to JSON strings.
- **Observable consequence:** Python can persist a different feature/context/row identity from the mapping the caller supplied instead of returning the stable boundary validation error.
- **Decision-complete correction:** After selecting a mapping or `dataclasses.asdict` result and before the existing `json.dumps`, iterate only its top-level keys and reject the first non-`str` key through `invalid_argument`. Preserve direct Pydantic `model_dump_json()` handoff and existing `allow_nan=False` behavior.
- **Focused closure proof:** Python boundary tests reject integer, float, boolean, and `None` keys for mappings/dataclasses before writer lookup, and accept a string-key mapping unchanged.

### `FIND-TASK-002-4`

- **Wave 1 source IDs:** `TASKREV-004`, `DC-2`
- **Status:** `CONFIRMED`
- **Classification:** `INCORRECT`
- **Violated obligation:** REQ-124 and `run_api.md` require TypeScript unsupported values, non-finite numbers, and unsafe integers to fail before queue admission instead of being omitted or coerced.
- **Exact location:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1068-1079,1105-1111,1123-1132,1144-1146`; all native observation calls consume those strings in `native/src/cards.rs:445-518`.
- **Evidence:** Raw `JSON.stringify` silently drops unsupported object members, converts `NaN`/infinities to `null`, and cannot preserve unsafe integer intent. Rust receives only the already-mutated JSON text.
- **Observable consequence:** Drift features, Eval evidence, media, or generic rows may be enqueued with content different from the caller's value while the call reports success.
- **Decision-complete correction:** Add one TypeScript boundary serializer reused by `drift`, `eval`, `record`, and `mediaJson`. Recursively validate the supplied value before one `JSON.stringify`, rejecting non-finite or unsafe numbers, `undefined`, functions, symbols, bigint, cycles, and other values that stringify by omission/coercion. Preserve deliberately absent optional media properties by constructing them only when present. Add no dependency.
- **Focused closure proof:** TypeScript unit tests prove nested unsupported values, non-finite values, unsafe integers, unsupported roots, and cycles raise the existing structured validation error before any native method is invoked; valid JSON values make exactly one native call.

### `FIND-TASK-002-5`

- **Wave 1 source IDs:** `TASKREV-005`, `RS-003`, `DC-4`
- **Status:** `CONFIRMED`
- **Classification:** `MISSING`
- **Violated obligation:** REQ-129, AC-026, and TASK-002 Scenario 4 require every first-class SDK to prefer explicit IDs and otherwise attempt active OpenTelemetry span capture when its runtime exposes one.
- **Exact location:** Rust fallback `crates/shared/wyrd-client/src/observe/eval.rs:123-141,192-214`; Python boundary `sdks/wyrd-sdk-python/src/observe/mod.rs:183-202`; TypeScript boundary `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1123-1132` and native pass-through `sdks/wyrd-sdk-ts/native/src/cards.rs:466-498`.
- **Evidence:** Python and TypeScript pass omitted IDs directly to Rust. Their runtime-local OpenTelemetry contexts do not become Rust's current `tracing` span across PyO3/N-API.
- **Observable consequence:** Eval observations inside active Python or Node spans persist null trace/span IDs unless callers manually copy them, breaking the promised trace join.
- **Decision-complete correction:** Keep Rust's existing fallback for Rust callers. At each foreign boundary, only when both explicit IDs are absent, query that runtime's installed OpenTelemetry API, accept both IDs only from a valid active span, and pass them through `EvalObservationOptions::from_parts`; an unavailable API or absent/invalid span supplies neither. Explicit IDs continue to win and explicit span-without-trace remains invalid. Reuse the already-declared Python OTEL extra and installed TypeScript `@opentelemetry/api`; add no dependency.
- **Focused closure proof:** Owning-runtime Python and Node tests install a real active span, omit explicit IDs, and assert the exact IDs reach the native call/persisted row; companion cases prove explicit IDs override the active span and absence remains null.

### `FIND-TASK-002-6`

- **Wave 1 source IDs:** `TASKREV-006`, `CONC-002`
- **Status:** `REVISED`
- **Classification:** `INCORRECT`
- **Violated obligation:** REQ-128, AC-025, and TASK-002 Scenario 6 require concurrent first use of a dynamic table to converge on one first-use describe and the existing producer.
- **Exact location:** `crates/shared/wyrd-client/src/bifrost/facade.rs:279-315`; its production callers are fixed-table startup at `observe/lifecycle.rs:171-175` and dynamic `Observe::record_value` at `observe/mod.rs:248-260`.
- **Evidence:** The cache lock is released before `TableConfig::describe`; every racing miss can perform HTTP metadata IO. The only test (`observe/tests.rs:686-717`) is sequential.
- **Observable consequence:** a first-write burst duplicates authenticated table reads and their audit decisions, violating the one-describe acceptance evidence and multiplying latency/load.
- **Decision-complete correction:** Add one async cache-miss gate owned by `Bifrost`. On a miss, acquire it, check the existing described-table cache again, perform one describe only if still absent, and insert into that same cache. Keep the existing `WriterPool` as the sole producer cache. Do not add a per-key single-flight type, another schema map, or producer cache; serializing rare first misses is the simpler sufficient ceiling.
- **Focused closure proof:** A barrier-controlled test launches concurrent records for one FQN and observes exactly one describe plus one producer; a two-FQN case proves explicit destinations/correlation remain distinct.

### `FIND-TASK-002-7`

- **Wave 1 source IDs:** `RS-002`, `DC-1`, `CONC-001`
- **Status:** `CONFIRMED`
- **Classification:** `INCORRECT`
- **Violated obligation:** REQ-133 and TASK-002 Scenario 1 require every successful shutdown to leave the state permanently closed and prevent an in-flight start from publishing a writer afterward.
- **Exact location:** `crates/shared/wyrd-client/src/observe/lifecycle.rs:78-141,159-199`; exposed solely through `WyrdState::{start_bifrost,start_bifrost_with,shutdown}` and their Python/TypeScript projections.
- **Evidence:** `shutdown` converts all `started()` errors to success without changing `NotStarted` or `Starting`; `StartClaim::complete` later assigns `Started`, and an uncompleted claim's `Drop` assigns `NotStarted` even if shutdown should have closed the lifecycle.
- **Observable consequence:** shutdown can return success and the same state can then restart or publish a live writer after the reported durability/teardown boundary.
- **Decision-complete correction:** Keep the existing `BifrostLifecycle` phase owner. Under its mutex, make never-started shutdown transition to `Closed`; make claim completion publish only from `Starting`; make claim drop restore `NotStarted` only from `Starting`; and fence a shutdown/start race so a successful shutdown leaves `Closed` and the losing start cannot install a writer. Preserve `Started` after an actual writer drain error so same-handle retry remains possible. No new lifecycle type or public state is needed.
- **Focused closure proof:** Deterministic tests prove shutdown-before-start rejects restart and a paused start/shutdown interleaving cannot publish after shutdown succeeds; existing started-success and failed-start retry tests remain green.

### `FIND-TASK-002-8`

- **Wave 1 source IDs:** `CONC-003`
- **Status:** `CONFIRMED`
- **Classification:** `MISSING`
- **Violated obligation:** REQ-133 and TASK-002 Scenario 1 explicitly require state-level proof that ambiguous shutdown is retried on the same handle without stranded rows.
- **Exact location:** public lifecycle tests `crates/shared/wyrd-client/src/observe/tests.rs:413-443`; lower-level retained retry exists in `bifrost/handle.rs:267-315` and `wyrd-queue/src/producer.rs:836-887`.
- **Evidence:** No test injects an ambiguous sink result through `WyrdState::shutdown`, observes the state remain on its original `StartedBifrost`, and retries that state to terminal closure.
- **Observable consequence:** replacement, premature closure, lost retained identity, or failure to close after retry could regress while every task-recorded state lifecycle test remains green.
- **Decision-complete correction:** Reuse the existing mock sink and `adopt_started_bifrost_for_test` seam. Add one state-level test that enqueues a row, forces one ambiguous shutdown result, retries `shutdown` on the same `WyrdState`, proves the retained batch identity settles, then proves writes and restart are refused. Do not add a new harness or language-specific duplicate for this Rust-owned lifecycle mechanic.
- **Focused closure proof:** The exact state-level test above fails before correction and passes with the same producer/batch identity across both shutdown calls.

### `FIND-TASK-002-9`

- **Wave 1 source IDs:** `RS-001`, `SEC-AUD-001`
- **Status:** `REVISED`
- **Classification:** `REGRESSION`
- **Violated obligation:** `architecture/agent-rules.md` requires a callee borrowing `TenantConn` to preserve caller-owned transaction lifecycle; audit publication failures must remain explicit and retryable.
- **Exact location:** `crates/vala/vala-sql/src/queries/audit_staging.rs:199-300`; sole production caller `crates/wyrd/wyrd-server/src/audit/publication.rs:348-367`.
- **Evidence:** after `SET LOCAL lock_timeout`, a timed-out `FOR UPDATE` returns `55P03` and aborts PostgreSQL's transaction. The helper converts it to `Ok(None)`, but `AuditPublisher::freeze` immediately commits that unusable transaction and receives an error. Other callers likewise receive apparent success with a transaction that cannot continue.
- **Observable consequence:** the SQL contract lies about success and shifts the real lock-timeout failure to an unrelated later statement/commit. Production currently warns through the existing staging-error path, so the Wave 1 claim of a silent successful `Idle` cycle is rejected.
- **Decision-complete correction:** Keep the three-second transaction-local timeout and delete the `55P03 -> Ok(None)` conversion. Map it through the existing `SqlError`/`AuditPublicationError::Staging` path so the caller rolls back on drop and the next sweep retries unchanged staging/bound state. Do not add a savepoint or new publication outcome merely to preserve the false idle result.
- **Focused closure proof:** A Postgres test holds the tenant chain head beyond the timeout, asserts a bounded explicit error (not `None`), drops/rolls back the failed transaction, releases the lock, and proves a fresh transaction freezes the unchanged owed range. The publisher-level assertion confirms the existing failure log/error path, not `PublishOutcome::Idle`.

### `FIND-TASK-002-10`

- **Wave 1 source IDs:** `RS-005`
- **Status:** `CONFIRMED`
- **Classification:** `VIOLATION`
- **Violated obligation:** `architecture/agent-rules.md` makes intent-bearing rustdoc for every new or materially modified Rust module/item a hard pre-merge rule.
- **Exact location:** `crates/vala/vala-bifrost-redux/src/tables/drift/result_features.rs:1`; `tables/eval/mod.rs:1-5`; `tables/eval/observations.rs:1`; `tables/eval/result_items.rs:1`; `tables/verification/mod.rs:1-3`; `tables/verification/results.rs:1`; parent module items at `tables/mod.rs:18,25`.
- **Evidence:** Each new module file starts with imports or bare declarations rather than `//!` module documentation; the new nested module items/re-exports have no item documentation independent of the well-documented table structs.
- **Observable consequence:** the candidate violates an explicit repository completion gate even though compilation and Clippy do not enforce private-item/module documentation.
- **Decision-complete correction:** Add concise module/item rustdoc describing each table module's ownership and schema role. Do not add lint suppressions, duplicate the full schema authority, or refactor the table types.
- **Focused closure proof:** Source audit of every added Rust item plus the existing format/lint lanes shows no undocumented new item and no suppression.

### `FIND-TASK-002-11`

- **Wave 1 source IDs:** `RS-006`
- **Status:** `CONFIRMED`
- **Classification:** `VIOLATION`
- **Violated obligation:** `architecture/agent-rules.md` requires module-top imports and bare imported types in signatures, outside its two narrow exceptions.
- **Exact location:** `crates/shared/wyrd-client/src/observe/eval.rs:198-200`; `crates/vala/vala-bifrost-redux/src/tables/mod.rs:907`; `crates/shared/wyrd-client/src/observe/lifecycle.rs:63-65`; `sdks/wyrd-sdk-python/src/observe/mod.rs:17`.
- **Evidence:** The full bodies contain ordinary function-scoped trait/helper imports and fully qualified `std::fmt` types in production signatures; none is the permitted single-generic-function trait exception.
- **Observable consequence:** the candidate violates the repository's explicit dependency-visibility rule.
- **Decision-complete correction:** Move the existing OpenTelemetry traits and table-field helpers to their module/test-module import blocks, and import `Debug`, `Formatter`, a `fmt::Result` alias, and `Display` for bare use in signatures. Change no behavior.
- **Focused closure proof:** `rg`/source audit of the complete added lines finds no disallowed function-scoped import or fully qualified signature type; format and lints pass.

### `FIND-TASK-002-12`

- **Wave 1 source IDs:** `SEC-TEN-001`
- **Status:** `REVISED`
- **Classification:** `MISSING`
- **Violated obligation:** REQ-128 and AC-025 require real-boundary evidence that unknown or unauthorized dynamic tables fail before queue admission; repository testing rules do not permit a mock-only unit test to replace that user-visible journey proof.
- **Exact location:** Rust journey `sdks/wyrd-sdk-rust/tests/observe_run.rs:295-398`; Python journey `sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py:280-327`; TypeScript journey `sdks/wyrd-sdk-ts/wyrd/tests/integration/observe-run.test.ts:131-265`; mock-only unknown case `crates/shared/wyrd-client/src/observe/tests.rs:752-766`.
- **Evidence:** The three real journeys use an admin-capable registered-service credential and exercise only the SDK-local reserved-table guard. Unknown-table behavior is proved only by a stub 404; no real SDK-to-server case proves a describe denial occurs before any row admission. Shared Scribe journeys already prove publisher/tenant stamping and signed Card-scope refusal, and AC-030 owns the full authorization/audit matrix, so those bundled Wave 1 requests are not retained here.
- **Observable consequence:** the explicit AC-025 negative boundary could regress in the real HTTP/auth/catalog path while TASK-002's recorded journeys remain green.
- **Decision-complete correction:** Extend the existing real SDK journeys with the smallest unknown-table call and stable not-found assertion before shutdown, and use one existing real-server credential/role seam to prove a denied `bifrost_table:read` describe returns the stable authorization error with no producer/admission. Assert the canonical describe allow/deny audit rows only at that permission boundary. Do not invent object-specific table authorization, repeat AC-030's full matrix, or add a new harness.
- **Focused closure proof:** The three SDK journey lanes each prove their public projection surfaces the real unknown-table refusal; one existing server-backed journey proves the denied describe and its audit row before enqueue. Existing positive two-table writes remain green.

## Validation result

Twelve bounded findings remain: `FIND-TASK-002-1` through
`FIND-TASK-002-12`. `RS-004` is rejected because its proposed atomic admission
directly conflicts with the approved row-at-a-time queue contract. No authority
conflict remains unresolved, every retained correction stays with an existing
owner/mechanism, and no finding requires specification revision.
