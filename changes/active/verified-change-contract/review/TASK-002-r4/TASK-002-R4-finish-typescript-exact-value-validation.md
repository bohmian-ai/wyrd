# TASK-002-R4: Finish TypeScript exact-value validation

Route this remediation directly to `$wyrd-implement`.

## Authority and cumulative subject

- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md`
- Original base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Reviewed cumulative candidate: `b56560e511918efbdd84d8756b13100fc381eda0`
- Prior remediation: `changes/active/verified-change-contract/review/TASK-002-r1/` through `TASK-002-r3/`
- R4 validation: `changes/active/verified-change-contract/review/TASK-002-r4/findings-validation.md`
- Logic authority: `changes/active/verified-change-contract/architecture/logic/run_api.md`
- Repository authorities: `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `architecture/bifrost-design.md`, and applicable routed references

A later review must reassess the complete cumulative candidate from the
original base and preserve the stable finding ID below. AI co-author trailers
are explicitly allowed by current user authority and are not remediation scope.

## Diagnosis and correction

### `FIND-TASK-002-4` — TypeScript can still admit a value different from the caller input

REQ-124, REQ-129, `run_api.md`, and the approved prior remediation outcome
require TypeScript observation inputs either to cross the native boundary
unchanged or to fail before admission when JavaScript would silently omit or
coerce data. The R3 correction rejects hidden and symbol-keyed object data and
named or symbol-keyed array data, but two reachable paths still violate the
same requirement.

First, `strictJson` at `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1100-1151` validates
property values through `Object.entries` or indexed array reads, then calls
`JSON.stringify` on the original caller object. An enumerable getter can
therefore return one accepted value during validation and a different or
omitted value during serialization. A public `Observe.eval` reproduction read
the getter twice, accepted `"yes"` on the first read, and invoked native with
`{}` after the second read returned `undefined`.

Keep `strictJson` as the sole serializer, but have its existing recursive
traversal build the exact plain JSON snapshot that is ultimately stringified,
so every accepted object property and array element is read once. Preserve the
current primitive, prototype, cycle, key, safe-number, optional-media, and
structured-error behavior; do not add another serializer or dependency.

Second, `mediaJson` at `index.ts:1184-1196` maps each caller-supplied media
descriptor to a new known-field object before `strictJson` can inspect the
original. Extra enumerable keys, symbol keys, and non-enumerable keys therefore
disappear while the Eval call succeeds. Validate each original descriptor's
own keys against the closed `id`, `kind`, `uri`, and optional `mediaType` shape
before converting `mediaType` to `media_type`. Reject symbol, non-enumerable,
or undeclared keys through the existing `WYRD_SPEC_400_VALIDATION` path, and
read each accepted field only once before passing the projected snapshot to the
existing serializer. Do not add a generalized validation framework or move
durable media validation out of Rust.

## Intended outcome

Every TypeScript Drift, Eval, generic record, array element, and Eval media
descriptor is read into one validated snapshot and the exact accepted snapshot
is what reaches native code; data that the boundary would otherwise discard is
refused before any native call.

## Constraints and preserved behavior

- Preserve every behavior already closed by findings `1` through `3`, `5`
  through `14`, `16`, and `17`.
- Preserve one TypeScript serializer, one state-owned `Bifrost`, the existing
  bounded queue, cache, producer pool, transport, stable error, and Rust-owned
  durable validation.
- Preserve explicit trace precedence, active-span fallback, fixed schemas,
  managed identity columns, writer/subject separation, and all completed SDK
  journeys.
- Preserve accepted ordinary JSON values and the required `mediaType` to
  `media_type` projection.
- Do not weaken or suppress repository checks or hand-edit generated output.

## Non-goals

- No new serializer, dependency, public type, error code, queue, cache,
  producer pool, transport, schema authority, lifecycle state, authorization
  model, or generalized validation framework.
- No Rust duplicate of the TypeScript trust-boundary validation.
- No unrelated TypeScript API refactor, SDK journey expansion, or test harness.
- No synchronous verdict wait, per-observation flush, multi-row API, run
  registry, observation type, or retention policy.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-002-4` | Getter-backed object fields and array elements are read once and native receives exactly the validated first value, while original media descriptors with symbol, non-enumerable, or undeclared keys fail with `WYRD_SPEC_400_VALIDATION` before any native call. |

## Focused and broader proof

Extend the existing `observe.test.ts` table with root and nested enumerable
getters plus accessor-backed array elements whose later reads would differ.
Exercise the applicable Drift, Eval, and record calls, assert each accessor is
read once, and assert native receives the exact first accepted value or the
existing structured refusal when that first value is unsupported.

Add Eval media cases with an extra enumerable key, a symbol key, a
non-enumerable key, and an accessor-backed optional field. Prove undeclared or
hidden data is refused before native admission and accepted declared fields are
read once and projected exactly.

Run the exact existing `observe.test.ts` Vitest target, followed by:

```bash
mise run ts:test:unit
mise run ts:typecheck
mise run ts:test:integration
mise run verify:bifrost
mise run fmt
mise run lints
git diff --check
```

Retain the previously recorded queue, lifecycle/cache, audit, denied-describe,
schema-fingerprint, and SDK journey proofs; no new harness is required.
