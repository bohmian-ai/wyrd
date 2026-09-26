# TASK-002-R1: Close scoped-observation contract gaps

Route this remediation directly to `$wyrd-implement`.

## Authority and immutable subject

- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md`
- Original review: `changes/active/verified-change-contract/review/TASK-002-r1/`
- Base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Reviewed candidate: `fbfc2591a985b288935180098f892aecdf3b8b49`
- Remediation reviews the later cumulative candidate against the same base and original task.
- Interface authorities: `changes/active/verified-change-contract/architecture/logic/run_api.md` and `table_schema.md`
- Repository authorities: `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `architecture/bifrost-design.md`, and applicable routed references.

## Diagnosis and required correction

### `FIND-TASK-002-1` — configured Rust startup API drift

REQ-123 and the locked Rust interface require
`WyrdState::start_bifrost_with_config(&WyrdClient, Option<TableConfig>, QueueConfig)`.
The candidate exposes only `start_bifrost_with(&client, table)` at
`crates/shared/wyrd-client/src/state.rs:440-466` and supplies
`QueueConfig::default()` internally. Because the Rust SDK only re-exports the
shared client, the approved example cannot compile and callers cannot configure
the existing bounded producer through state-owned startup. The current journey
uses only the default form, so its green result does not prove the locked API.

Restore the exact configured method on `WyrdState`, route it through the
existing lifecycle claim and `Bifrost::connect_with_config`, and re-export the
existing `wyrd_queue::QueueConfig` through the shared-client/Rust-SDK surface.
The default-config convenience may remain as a thin call to this method. Do not
create another config type or constructor vocabulary.

### `FIND-TASK-002-2` — incomplete fixed-table compatibility check

REQ-127, AC-025, and `table_schema.md` lock each fixed table's complete authored
field sequence. `require_projection` at
`crates/shared/wyrd-client/src/observe/mod.rs:305-331` looks fields up by name
and compares only datatype. It accepts extra fields, reordered fields, and
wrong nullability, so startup can claim compatibility with a stale schema and
defer failure until sealing. The existing wrong-datatype test cannot reveal
these cases.

Keep compatibility ownership in the existing shared projection check. Make
the Drift and Eval declarations include expected nullability and compare the
described authored fields as one exact ordered sequence: count, name, datatype,
and nullability. Managed columns remain Bifrost-owned and outside this client
comparison. Do not create a second schema authority.

### `FIND-TASK-002-3` — Python silently coerces mapping keys

REQ-124 requires Python mapping and dataclass reductions to use string keys.
`json_text` at `sdks/wyrd-sdk-python/src/observe/mod.rs:37-75` passes mappings
and `dataclasses.asdict` output to `json.dumps(allow_nan=False)` without key
inspection. Python stringifies integer, float, boolean, and `None` keys before
Rust can reject them, changing feature or evidence identity while reporting
success. Existing tests cover valid values and non-finite numbers, not key
coercion.

After selecting a mapping or dataclass mapping and before the existing JSON
dump, inspect its top-level keys and reject the first non-`str` key through the
existing structured invalid-argument path. Preserve direct Pydantic
`model_dump_json()` handoff and `allow_nan=False`; do not duplicate Rust record
validation in Python.

### `FIND-TASK-002-4` — TypeScript silently omits or coerces values

REQ-124 and `run_api.md` require TypeScript to refuse unsupported values,
non-finite numbers, and unsafe integers before native admission. The observation
and media paths at `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1068-1146` call raw
`JSON.stringify`. JavaScript drops `undefined`, functions, and symbols in
objects; converts non-finite numbers to `null`; and cannot preserve unsafe
integer intent. Rust receives only the mutated JSON text. The valid-only tests
therefore cannot prove the boundary.

Use one TypeScript-owned strict JSON serializer for Drift, Eval, generic
records, and media. Validate recursively before one `JSON.stringify`, rejecting
non-finite or unsafe numbers, unsupported roots or nested values, bigint, and
cycles through the existing structured validation error. Preserve intentionally
absent optional media fields by constructing them only when present. Add no
dependency and do not move durable validation out of Rust.

### `FIND-TASK-002-5` — foreign runtimes do not capture active spans

REQ-129, AC-026, and Scenario 4 require explicit trace/span IDs to win, then a
best-effort lookup from the active span exposed by each runtime. Rust implements
its own `tracing` fallback in
`crates/shared/wyrd-client/src/observe/eval.rs:192-214`, but Python
`sdks/wyrd-sdk-python/src/observe/mod.rs:183-202` and TypeScript
`sdks/wyrd-sdk-ts/wyrd/src/index.ts:1123-1132` merely pass absent IDs through.
Python and Node OpenTelemetry contexts do not become Rust's current span across
PyO3 or N-API, so observations lose their trace join. Existing tests use
explicit IDs or no active runtime span.

Keep the Rust fallback. At each foreign boundary, only when both explicit IDs
are absent, read the active span from that runtime's existing OpenTelemetry API,
accept both IDs only from a valid context, and pass them through the existing
Rust options path. An unavailable API, absent span, or invalid context supplies
neither ID. Explicit IDs remain authoritative, and an explicit span without a
trace remains invalid. Reuse the declared Python OTEL extra and installed
TypeScript `@opentelemetry/api`; add no dependency.

### `FIND-TASK-002-6` — concurrent first use duplicates table description

REQ-128 and AC-025 require concurrent first use of a dynamic FQN to converge.
`Bifrost::writer_table` at
`crates/shared/wyrd-client/src/bifrost/facade.rs:279-315` releases the cache
lock before awaiting describe, so every racing miss can perform authenticated
metadata IO and emit its own audit decision. The only new test is sequential.

Keep `Bifrost` as the owner, its existing described-table map as the schema
cache, and `WriterPool` as the only producer cache. Add one owner-local async
cache-miss gate; after acquiring it, recheck the existing map and describe only
if still absent. Serializing rare first misses across FQNs is the approved
minimal ceiling. Do not add a per-key single-flight subsystem, second schema
map, or producer cache.

### `FIND-TASK-002-7` — successful shutdown is not terminal

REQ-133 requires every successful shutdown to close the state permanently.
`BifrostLifecycle::shutdown` at
`crates/shared/wyrd-client/src/observe/lifecycle.rs:78-141` converts
`NotStarted` and `Starting` into success without changing phase. Claim
completion can then publish `Started`, and claim drop can restore `NotStarted`,
so restart or post-shutdown publication remains possible. Existing tests even
treat shutdown-before-start as a no-op and do not control the race.

Keep the existing lifecycle phase owner. Under its mutex, make never-started
shutdown transition to `Closed`; permit claim completion or rollback only from
`Starting`; and fence the start/shutdown race so a successful shutdown remains
`Closed` and the losing start cannot install a writer. Preserve `Started` when
an actual writer drain is ambiguous or fails, allowing same-handle retry. Add
no new public lifecycle state or replacement writer.

### `FIND-TASK-002-8` — ambiguous state shutdown lacks its required proof

REQ-133 and Scenario 1 explicitly require retry on the same state-owned handle
after an ambiguous drain. Tests at
`crates/shared/wyrd-client/src/observe/tests.rs:413-443` cover only successful
started shutdown and never-started shutdown. Lower-level producer tests retain
batches, but they do not prove `WyrdState` preserves the same
`StartedBifrost`, producer, and batch identity across the public retry.

Reuse the existing mock sink and `adopt_started_bifrost_for_test` seam. Drive
one queued row through an ambiguous `WyrdState::shutdown`, retry shutdown on the
same state, prove the retained batch identity settles, then prove writes and
restart are refused. This Rust-owned lifecycle mechanic needs no new harness or
language-specific duplicate.

### `FIND-TASK-002-9` — audit lock timeout returns success from an aborted transaction

The caller owns a borrowed `TenantConn` transaction, and the SQL helper must
return an honest composable result. In
`crates/vala/vala-sql/src/queries/audit_staging.rs:199-300`, PostgreSQL `55P03`
from the bounded `FOR UPDATE` aborts the transaction, but the helper converts it
to `Ok(None)`. The sole publisher caller then fails while committing the
unusable transaction, shifting the real error to an unrelated boundary. The
timeout test does not prove transaction composability afterward.

Keep the three-second transaction-local timeout. Remove only the
`55P03 -> Ok(None)` conversion and propagate it through the existing
`SqlError`/`AuditPublicationError::Staging` path so the failed transaction
rolls back on drop and the next sweep retries unchanged state. Do not add a
savepoint, outcome variant, owner token, lease, or alternate publisher path.

### `FIND-TASK-002-10` — mandatory Rust module documentation is absent

The new table modules at
`tables/drift/result_features.rs`, `tables/eval/{mod,observations,result_items}.rs`,
and `tables/verification/{mod,results}.rs` under
`crates/vala/vala-bifrost-redux/src/`, plus their new parent module items in
`tables/mod.rs`, lack the intent-bearing rustdoc required by
`architecture/agent-rules.md`. Compilation and Clippy do not cover this hard
repository gate.

Add concise module/item rustdoc that states each module's table ownership and
schema role. Do not duplicate the complete schema authority, add suppressions,
or refactor the table types.

### `FIND-TASK-002-11` — import and signature placement violations

Function-scoped imports remain in
`crates/shared/wyrd-client/src/observe/eval.rs:198-200` and
`crates/vala/vala-bifrost-redux/src/tables/mod.rs:907`; fully qualified
signature types remain in
`crates/shared/wyrd-client/src/observe/lifecycle.rs:63-65` and
`sdks/wyrd-sdk-python/src/observe/mod.rs:17`. None fits the repository's narrow
exceptions. Green formatting and lints do not enforce this explicit rule.

Move the existing OpenTelemetry traits and table field helpers into their
module or test-module import blocks. Import the formatting/debug/display types
and use their bare names or an imported result alias in signatures. Change no
behavior and add no abstraction.

### `FIND-TASK-002-12` — real unknown/unauthorized describe evidence is missing

REQ-128 and AC-025 require real-boundary proof that an unknown or unauthorized
dynamic table fails before queue admission. The Rust, Python, and TypeScript
journeys exercise only the SDK-local reserved-name guard; the unknown-table case
at `crates/shared/wyrd-client/src/observe/tests.rs:752-766` uses a mock 404.
Shared Scribe journeys already prove publisher stamping and Card-scope denial,
and AC-030—not TASK-002—owns the full auth matrix, so duplicating those suites
would broaden remediation.

Extend each existing SDK journey with the smallest real unknown-table call and
stable not-found assertion. Use one existing real-server credential/role seam
to prove a denied `bifrost_table:read` describe returns the stable authorization
error, produces the canonical permission audit evidence, and reaches no
producer/admission. Do not invent object-specific table authorization, repeat
AC-030's full matrix, or create a new harness.

## Intended outcome

The cumulative candidate matches revision 32 exactly: its public Rust startup
is callable as documented; fixed schemas are checked exactly; Python and
TypeScript cannot mutate observation input during serialization; runtime-local
active spans correlate Eval rows; concurrent dynamic first use performs one
describe; shutdown is terminal yet retryable after ambiguity; audit timeout
failure is explicit at its owner; repository documentation/import rules pass;
and the required dynamic-table negative paths are proven across real boundaries.

## Constraints and preserved behavior

- Preserve the five approved table schemas, managed columns, identity split,
  partitions, Bloom floors, and retention behavior.
- Preserve one `Bifrost` lifetime per `WyrdState`, one existing bounded queue,
  `WriterPool` as the sole producer cache, and explicit immutable destinations.
- Preserve row-at-a-time Drift admission. One logical multi-feature observation
  is not an atomic queue operation.
- Preserve explicit trace/span precedence and the invalid span-without-trace
  rule.
- Preserve ambiguous producer state and batch identity for same-handle retry.
- Preserve the canonical audit staging path, monotonic watermark/frozen bound,
  and publisher ownership.
- Keep durable server behavior in Rust and foreign-language code limited to its
  runtime boundary.
- Do not hand-edit generated stubs or schemas; regenerate through the owning
  source and repository tasks.
- Do not weaken checks, add lint suppressions, or replace required real journeys
  with mocks.

## Non-goals

- No new queue, transport, schema cache, producer cache, config type, or
  lifecycle vocabulary.
- No synchronous verdict/scoring wait, observation flush, run registry, durable
  observation type, or verification-specific retention policy.
- No atomic multi-row producer API for a Drift observation.
- No per-FQN single-flight framework unless the minimal owner-wide miss gate is
  proven insufficient by the required tests.
- No new authorization model, object-specific table ACL, audit sink, lease,
  owner token, or publisher outcome.
- No repetition of AC-030's complete authorization matrix.
- No unrelated refactor or documentation expansion.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-002-1` | `wyrd_sdk::QueueConfig` is publicly usable and the exact locked configured startup call compiles and reaches existing configured construction. |
| `FIND-TASK-002-2` | Fixed startup rejects reordered, extra, wrong-type, and wrong-nullability authored fields and accepts both canonical schemas. |
| `FIND-TASK-002-3` | Python mapping/dataclass inputs reject integer, float, boolean, and `None` keys before writer lookup; string keys round-trip unchanged. |
| `FIND-TASK-002-4` | TypeScript rejects every specified omission/coercion case before native invocation and calls native exactly once for valid JSON. |
| `FIND-TASK-002-5` | Python and Node active spans supply exact IDs only when explicit IDs are absent; explicit IDs win and no span remains null. |
| `FIND-TASK-002-6` | Concurrent first use of one FQN observes one describe and one producer; two explicit FQNs remain distinct. |
| `FIND-TASK-002-7` | Shutdown-before-start and controlled start/shutdown races end in permanent closure; failed drains remain retryable. |
| `FIND-TASK-002-8` | One state-level test proves the same producer/batch settles across ambiguous shutdown and retry, then refuses writes/restart. |
| `FIND-TASK-002-9` | A held chain-head lock yields a bounded explicit error, rollback leaves the range unchanged, and a fresh retry succeeds through the existing publisher path. |
| `FIND-TASK-002-10` | Every added Rust module/item has the required intent-bearing rustdoc without suppression. |
| `FIND-TASK-002-11` | Added production code has no disallowed function-scoped import or fully qualified signature type. |
| `FIND-TASK-002-12` | All three SDK journeys surface real unknown-table refusal; one server-backed path proves denied describe plus canonical audit evidence before enqueue. |

## Verification

Run the focused tests introduced or extended for every acceptance criterion,
using exact `mise exec -- cargo nextest run --locked ... -E 'test(=...)'`
commands for specifically named Rust tests and the repository-managed Postgres
wrapper where required. Record the exact names and results in TASK-002.

Then run the narrow complete closure already established for this capability:

```bash
mise run verify:bifrost
mise run test:shared
mise run test:wyrd-sdk
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

Also rerun the focused `wyrd-queue` fixed-size-binary regression recorded by
the original task to prove the unrelated shared conversion path remains intact.
