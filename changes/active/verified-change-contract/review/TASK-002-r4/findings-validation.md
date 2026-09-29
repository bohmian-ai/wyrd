# TASK-002 R4 structured Ponytail validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verified-change-contract`
- Base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Candidate: `b56560e511918efbdd84d8756b13100fc381eda0`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md`
- Prior review authority: `review/TASK-002-r1/`, `review/TASK-002-r2/`, and
  `review/TASK-002-r3/`, including their verdicts, validated ledgers, and
  remediation tasks

The complete cumulative diff, all six R4 Wave 1 reports, applicable
authorities, prior review artifacts, and the current caller and test bodies
were inspected. The repository has no `.codegraph/` directory, so caller
tracing used `rg`, direct source reads, immutable diff inspection, and a public
built-SDK probe. `HEAD` resolved to the candidate before validation and again
before this report was written. Per explicit current user authority, AI
co-author trailers are allowed and are not findings.

## Wave 1 proposal validation

| Proposal | Decision | Independent validation and minimum correction |
|---|---|---|
| `TASKREV-R4-001` | **CONFIRMED**, reopening `FIND-TASK-002-4` | `strictJson` validates plain-object values through `Object.entries` and array elements through indexed reads, then calls `JSON.stringify` on the original value. A public `Observe.eval` probe with an enumerable `answer` getter returned `"yes"` to validation and `undefined` to serialization; the getter was read twice and native received `{}` successfully. `Observe.drift`, `Observe.eval`, and `Observe.record` all call this owner directly. Keep the existing serializer, but build and stringify the JSON-compatible value produced by its validation traversal so each accepted property or element is read once; add no second serializer or dependency. |
| `DATA-R4-001` | **CONFIRMED**, duplicate of `TASKREV-R4-001` | Independent source tracing and the same public probe reproduce the data-boundary consequence: accepted TypeScript data can differ from the JSON handed toward Bifrost. It is deduplicated into the accessor branch of `FIND-TASK-002-4`; no separate stable finding is warranted. |
| `DC-R4-001` | **REVISED**, merged into `FIND-TASK-002-4` | `mediaJson` maps each caller descriptor to a new known-field object before `strictJson` runs. Public probes showed an extra enumerable property, a symbol key, and a non-enumerable property all disappear while native receives a successful media call. The exact `EvalMediaRef` contract and the previously approved outcome that no own TypeScript input property disappear make this a reachable sibling of the same stable finding, not a new contract. Before camelCase-to-snake-case projection, inspect the original descriptor's own keys and reject symbol, non-enumerable, or undeclared keys through the existing validation error; read each accepted field once and pass the projected value through the existing serializer. |

The contracts proposal is revised only in correction shape: running
`strictJson` over the original media object would not reject an enumerable
unknown string key and would serialize the wrong `mediaType` wire name. A
small declared-key check at the existing `mediaJson` projection boundary is
the sufficient correction; a new serializer, dependency, public type, or Rust
validation layer is unnecessary.

## Deduplicated final finding ledger

### `FIND-TASK-002-4` — TypeScript can still admit observation data different from the caller input

- **Wave 1 source IDs:** `TASKREV-R4-001`, `DATA-R4-001`, `DC-R4-001`
- **Status:** REVISED / REOPENED
- **Classification:** INCORRECT
- **Violated obligation:** REQ-124, REQ-129, `run_api.md`, and the approved
  R1-R3 remediation outcome require unsupported TypeScript values and silent
  JavaScript omission or coercion to fail before native queue admission, with
  the accepted observation value preserved through the SDK boundary.
- **Exact location:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1100-1151`
  (`strictJson` validation followed by serialization of the original value),
  `index.ts:1184-1196` (`mediaJson` projection before strict inspection), and
  missing cases in
  `sdks/wyrd-sdk-ts/wyrd/tests/unit/observe.test.ts:134-196`.
- **Caller and reachability evidence:** `strictJson` has exactly four
  production call sites: `mediaJson`, `Observe.drift`, `Observe.eval`, and
  `Observe.record`. The latter three accept public caller values directly.
  The built candidate read an ordinary enumerable getter twice and invoked
  Eval native with `{}` after its first value passed validation. Separately,
  public Eval calls carrying media descriptors with an extra enumerable key,
  symbol key, or non-enumerable key all reached native with those properties
  removed because `mediaJson` created a replacement object first. These are
  active public paths, not dormant, test-only, concurrent, proxy, or
  server-dependent behavior.
- **Observable consequence:** a TypeScript caller can receive successful
  admission while context, Drift features, a generic row, an array element, or
  a media descriptor differs from the value the SDK accepted, leaving Rust and
  persisted evidence unable to detect the discarded or changed data.
- **Decision-complete correction:** Retain `strictJson` as the sole JSON
  serializer and existing `WYRD_SPEC_400_VALIDATION` owner. During its current
  recursive traversal, construct the exact plain JSON snapshot to stringify so
  every accepted object property and array element is read once. At
  `mediaJson`, validate each original descriptor's own keys against the closed
  `id`, `kind`, `uri`, and optional `mediaType` shape before the required
  `mediaType` to `media_type` projection, reject symbol, non-enumerable, and
  undeclared keys, and read accepted fields once. Preserve current primitive,
  prototype, cycle, safe-number, optional-media, and Rust-owned durable media
  validation behavior. Add no dependency, second serializer, Rust duplicate,
  public type, or generalized validation framework.
- **Focused closure proof:** Extend the existing TypeScript observation unit
  test with root and nested enumerable object getters and accessor-backed array
  elements whose later reads would differ, exercise the applicable Drift,
  Eval, and record paths, assert each accessor is read once, and assert native
  receives the exact accepted first value or the existing structured refusal.
  Add Eval media descriptors with an extra enumerable key, symbol key, and
  non-enumerable key, plus an accessor-backed optional field, and prove they
  are either preserved by the declared projection or refused before any native
  call. Run the exact `observe.test.ts` Vitest target, `mise run ts:test:unit`,
  `mise run ts:typecheck`, and `mise run ts:test:integration`.

## Prior-finding closure

| Stable finding | R4 validation status |
|---|---|
| `FIND-TASK-002-1` | CLOSED — configured Rust startup remains public and exercised. |
| `FIND-TASK-002-2` | CLOSED — fixed schemas are compared completely and in order. |
| `FIND-TASK-002-3` | CLOSED — Python rejects non-string mapping and dataclass keys before serialization. |
| `FIND-TASK-002-4` | **OPEN / REOPENED** — R3's hidden, symbol, and non-index cases are fixed, but accessor double reads and pre-validation media projection remain reachable silent-change paths. |
| `FIND-TASK-002-5` | CLOSED — Python and Node project their own active spans with explicit identity precedence. |
| `FIND-TASK-002-6` | CLOSED — concurrent first-use describes converge through the existing owner gate and cache. |
| `FIND-TASK-002-7` | CLOSED — successful shutdown is terminal across never-started and in-flight-start states. |
| `FIND-TASK-002-8` | CLOSED — ambiguous drain retry retains the same state, writer, and batch identity. |
| `FIND-TASK-002-9` | CLOSED — audit lock timeout propagates honestly and preserves later retry. |
| `FIND-TASK-002-10` | CLOSED — the cumulative changed-item rustdoc remediation remains present. |
| `FIND-TASK-002-11` | CLOSED — `Range`, `OnceLock`, `AtomicU32`, `SocketAddr`, `Duration`, and `service_account_by_card_ref` are in the applicable module-top import groups and cited declarations use bare names. |
| `FIND-TASK-002-12` | CLOSED — real unknown and denied describes fail before admission with canonical audit proof. |
| `FIND-TASK-002-13` | CLOSED — every SDK Eval journey persists session/media and trace/span identity and proves both required refusals add no row. |
| `FIND-TASK-002-14` | CLOSED — real SDK journeys prove fixed-table startup refusal and cached reuse, and the shared real-server path proves stale-fingerprint fencing. |
| `FIND-TASK-002-16` | CLOSED — `EvalMediaRef` and `EvalOptions` are readonly object type aliases with unchanged fields and behavior. |
| `FIND-TASK-002-17` | CLOSED — the task evidence, helper, and test consistently name `WYRD_SPEC_400_VALIDATION`. |

The six Wave 1 reports and independent inspection found no remaining material
gap in the state-owned Bifrost lifetime, fixed schemas, bounded queue, Card
scope and writer/subject split, fixed-width decoding, describe/cache and
fingerprint fences, audit/tenancy behavior, Rust/Python projections, SDK
journeys, or explicit non-goals. The current user instruction supersedes the
repository default for AI co-author trailers, so no commit-metadata finding is
retained.

## Verification limits

The task records `verify:bifrost` passing all nine lanes at `d6231892`, after
the R3 code changes, plus the focused TypeScript unit test, TypeScript unit,
typecheck, and integration lanes, queue fixed-width tests, shared observation
tests, formatting, lints, Clippy-allow audit, and `git diff --check`. Wave 1
reran selected focused tests. Those green checks do not exercise accessor
double reads or own properties removed by `mediaJson`; both gaps were directly
reproduced against the built candidate through public `Observe.eval`.

## Overall validation result

**NON-EMPTY VALIDATED LEDGER.** One bounded stable finding remains:
`FIND-TASK-002-4`. Its two branches share the existing TypeScript boundary and
validation error and require no new product, public API, architecture,
security, compatibility, concurrency, resource-ownership, or persistent-data
decision. No specification revision is required.
