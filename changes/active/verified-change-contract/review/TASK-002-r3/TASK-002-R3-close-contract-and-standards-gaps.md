# TASK-002-R3: Close contract and standards gaps

Route this remediation directly to `$wyrd-implement`.

## Authority and cumulative subject

- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md`
- Original base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Reviewed cumulative candidate: `04f73570397d5123eb767abafa60d016c37de1db`
- Prior remediation: `changes/active/verified-change-contract/review/TASK-002-r1/` and `TASK-002-r2/`
- R3 validation: `changes/active/verified-change-contract/review/TASK-002-r3/findings-validation.md`
- Logic authorities: `changes/active/verified-change-contract/architecture/logic/run_api.md` and `table_schema.md`
- Repository authorities: `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `architecture/bifrost-design.md`, and applicable routed references

A later review must reassess the complete cumulative candidate from the
original base and preserve the stable finding IDs below.

## Diagnosis and correction

### `FIND-TASK-002-4` — TypeScript still silently discards own observation data

REQ-124 and the approved R1/R2 correction require the TypeScript boundary to
reject values that JavaScript serialization would silently omit or coerce
before native admission. The shared `strictJson` implementation at
`sdks/wyrd-sdk-ts/wyrd/src/index.ts:1094-1135` now rejects symbol keys on plain
objects, but arrays are inspected only through their numeric elements and
plain objects only through enumerable string entries. A public wrapper probe
therefore admitted an array with an own `extra` property and symbol property as
only `[1]`, and admitted a plain object with a non-enumerable own `hidden`
property without that property; both truncated values reached native code.

Keep `strictJson` as the sole serializer. Before traversing values, reject
plain-object non-enumerable own string keys as well as symbol keys, and reject
array-owned symbol keys or string keys other than `length` and canonical
in-range indices. Preserve the existing prototype, cycle, scalar, safe-number,
and optional-media behavior. Extend the existing table-driven refusal test for
root and nested shapes across Drift, Eval, and generic record calls, asserting
`WYRD_SPEC_400_VALIDATION` and zero native calls. Do not add another serializer,
dependency, or Rust-side duplicate.

### `FIND-TASK-002-11` — changed Rust declarations still violate import rules

`architecture/agent-rules.md` requires every dependency import at module top
and bare imported names in signatures, fields, statics, and constants. The
cumulative candidate adds `service_account_by_card_ref` to a late import block
and uses qualified `Range`, `OnceLock`, `AtomicU32`, and `SocketAddr` types in
new or changed `wyrd-testing/src/server.rs` declarations. The final Oracle test
changes similarly type new constants as `std::time::Duration` in
`wyrd-server/tests/pg_router_smoke.rs`. These are live paths used by the SDK
journeys, server startup, and cutoff test, so the prior import finding is not
fully closed.

Reuse the existing top import groups, move only the cited newly used symbols
there, remove `service_account_by_card_ref` from the late block, and use bare
names in the changed declarations. Preserve the reservation algorithm, Oracle
timing, and unrelated pre-existing expressions. Add no wrapper, refactor, or
new style check.

### `FIND-TASK-002-16` — two new TypeScript data shapes are open interfaces

The TypeScript guide requires object type aliases unless declaration merging
is needed. `EvalMediaRef` and `EvalOptions` at
`sdks/wyrd-sdk-ts/wyrd/src/index.ts:1044-1066` are closed observation input
shapes; repository-wide tracing finds no merge or extension consumer.

Replace only those two exported interfaces with readonly object type aliases,
preserving every field, optional marker, union, and documentation. Do not
change runtime conversion or generated native declarations.

### `FIND-TASK-002-17` — the committed evidence names the wrong error code

The R2 evidence row in
`changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md:397`
claims the symbol-key tests assert `WYRD_SDK_400_INVALID_OBSERVATION`, while
both `invalidObservationInput` and the passing unit assertion use
`WYRD_SPEC_400_VALIDATION`. The implementation correctly reused the existing
catalog-backed validation path; only the evidence is inaccurate.

Change that one evidence cell to `WYRD_SPEC_400_VALIDATION`. Do not change the
working error implementation to match the incorrect prose.

## Intended outcome

TypeScript observation inputs cannot lose any own data before native
admission; the two new Eval option shapes remain closed; all changed Rust
declarations expose dependencies through their module-top imports; and the
task evidence names the actual stable error.

## Constraints and preserved behavior

- Preserve every behavior already closed by findings `1` through `3`, `5`
  through `10`, and `12` through `14`.
- Preserve one TypeScript serializer, one state-owned `Bifrost`, the existing
  bounded queue, existing table cache, and `WriterPool` as sole producer cache.
- Preserve explicit trace precedence, runtime-local active-span fallback,
  fixed table schemas, managed identity columns, writer/subject separation,
  and the completed Rust/Python/TypeScript journey coverage.
- Preserve the test-server hooks as test-only and production audit publication
  behavior unchanged.
- Do not weaken or suppress checks, hand-edit generated artifacts, or add an
  enforcement script for these bounded style corrections.

## Non-goals

- No new serializer, dependency, config type, queue, cache, producer pool,
  transport, schema authority, lifecycle state, authorization model, or public
  error code.
- No per-language duplication of owner-level concurrency or stale-writer tests.
- No synchronous verdict wait, per-observation flush, atomic multi-row API,
  run registry, observation type, or retention policy.
- No unrelated import cleanup, TypeScript API refactor, documentation rewrite,
  or test harness.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-002-4` | Root and nested non-enumerable plain-object properties plus array-owned symbol/non-index properties fail with `WYRD_SPEC_400_VALIDATION` before any Drift, Eval, or record native call; valid JSON remains unchanged. |
| `FIND-TASK-002-11` | Every cited changed Rust dependency is in the existing module-top import group and every cited declaration uses the bare imported type, without changing runtime behavior. |
| `FIND-TASK-002-16` | `EvalMediaRef` and `EvalOptions` are readonly object type aliases with unchanged public fields and behavior. |
| `FIND-TASK-002-17` | The evidence row, serializer helper, and focused unit assertion all name `WYRD_SPEC_400_VALIDATION`. |

## Focused and broader proof

Run the exact existing `observe.test.ts` Vitest target after adding the refused
input rows, then run:

```bash
mise run ts:test:unit
mise run ts:typecheck
mise run ts:test:integration
mise run fmt
mise run lints
mise run check:clippy-allow-audit
git diff --check
```

Because the functional corrections touch the shared TypeScript authoring
surface, rerun `mise run verify:bifrost`
and retain the previously required focused queue, lifecycle/cache, audit, and
denied-describe proofs recorded in TASK-002.
