# TASK-002-R2: Close boundary and journey gaps

Route this remediation directly to `$wyrd-implement`.

## Authority and cumulative subject

- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md`
- Original base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Reviewed cumulative candidate: `a000c201ae86f584fd5b80349f375e087902fd78`
- R1 remediation: `changes/active/verified-change-contract/review/TASK-002-r1/TASK-002-R1-close-scoped-observation-gaps.md`
- R2 validation: `changes/active/verified-change-contract/review/TASK-002-r2/findings-validation.md`
- Logic authorities: `changes/active/verified-change-contract/architecture/logic/run_api.md` and `table_schema.md`
- Repository authorities: `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `architecture/bifrost-design.md`, and applicable routed references.

A later review must reassess the complete cumulative candidate from the original
base and preserve stable finding IDs.

## Diagnosis and correction

### `FIND-TASK-002-4` — TypeScript still drops symbol-keyed data

REQ-124 and `run_api.md` require unsupported input and silent JavaScript
omission to fail before native admission. The shared `strictJson` implementation
at `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1094-1132` validates plain objects with
`Object.entries`; own symbol keys appear in neither that traversal nor
`JSON.stringify`. An enumerable symbol-keyed property therefore disappears and
the native call receives only the remaining string-keyed data. R2 reproduced
this against the candidate, while the existing test covers a symbol value under
a string key rather than a symbol key.

Keep the single existing serializer. Before traversing a plain object's string
entries, reject any own symbol key through the existing structured invalid-input
path. Add root and nested own-symbol-key cases to the existing table-driven
serializer test and prove no native call occurs. Do not add another serializer,
dependency, or Rust-side duplicate.

### `FIND-TASK-002-10` — mandatory Rust documentation remains incomplete

`AGENTS.md` §16 and `architecture/agent-rules.md` require intent-bearing rustdoc
for every new or materially modified Rust item, including private/test items,
fields, associated constants, and methods. R1 added useful module and table
struct docs, but the new `DomainTable` implementations in
`tables/drift/result_features.rs`, `tables/eval/observations.rs`,
`tables/eval/result_items.rs`, and `tables/verification/results.rs` still have
undocumented associated items; the test-local `Features` type and `score` field
at `crates/shared/wyrd-client/src/observe/tests.rs:752-755` demonstrate another
miss. Clippy does not enforce this stronger repository rule.

Audit every Rust declaration added or materially changed in the complete
original-base-to-candidate range, then add concise documentation only where it
is missing. Include required `# Errors`, `# Panics`, cancellation, or
partial-progress sections when the item is fallible, panicking, or async. Do
not add a lint/check, suppression, wrapper, or structural refactor.

### `FIND-TASK-002-13` — the Eval journey contract is incomplete

TASK-002 Scenario 4 and AC-026 require the Rust, Python, and TypeScript Eval
journeys to exercise optional session/media data, preserve explicit or active
trace identity through the real SDK/server/queue/Arrow path, read back exact
`FixedSizeBinary(16)/(8)` values, and reject an invalid trace pair and malformed
media before admission. Rust currently uses default Eval options and reads no
trace/session/media columns; TypeScript does likewise and proves active-span
forwarding only against a fake native object; Python persists active/explicit
IDs but its journey omits session/media and the two required negative cases.
Unit tests cannot expose binding, fixed-width conversion, IPC, or persisted
readback regressions.

Extend the three existing observation journeys and reuse their servers, tables,
and queries. Rust uses a valid explicit trace/span pair, Python retains its
active/explicit cases, and TypeScript uses a real active pair across N-API.
Each journey supplies session/media, reads back the authored fields and exact
non-null trace/span bytes or canonical hex, and proves span-without-trace plus
malformed media adds no row. Use the existing public SDK validation surface;
add no harness, record type, or serializer.

### `FIND-TASK-002-14` — AC-025 real-boundary evidence is incomplete

AC-025 and TASK-002 Scenarios 1, 6, and 7 require real SDK journey evidence for
fixed-table startup refusal, no repeated dynamic-table schema IO after caching,
and stale-schema fingerprint refusal. The shared owner tests correctly prove
exact schema comparison and concurrent miss convergence, but each SDK journey
only starts against canonical built-ins and writes each dynamic table once.
None drives either fixed-table failure, observes cached reuse, or crosses the
server fingerprint fence with a stale writer. Lower-tier owner tests cannot
replace journey cases expressly assigned to the three SDKs.

Extend the existing Rust, Python, and TypeScript journeys with table-driven
failure for each fixed-table preflight and a repeated dynamic write whose
server-observed describe count remains one. Add stale-schema fingerprint refusal
to the narrowest existing real-server journey over the shared Rust owner and
assert the public stable error before a replacement schema is accepted. Keep
the shared barrier-controlled owner test as the sole concurrency/producer
uniqueness proof; do not copy it into every language or add cache/harness
abstractions.

## Intended outcome

TypeScript cannot silently discard any own input property; the cumulative Rust
change satisfies the repository's every-item documentation rule; all three Eval
journeys prove authored option data, persisted fixed-width correlation, and
negative admission; and AC-025's required startup/cache/fingerprint behavior is
proven through the existing real SDK boundaries.

## Constraints and preserved behavior

- Preserve the already closed behavior for findings `1` through `3`, `5`
  through `9`, `11`, and `12`.
- Preserve one TypeScript serializer, one state-owned `Bifrost`, the existing
  bounded queue, the existing table cache, and `WriterPool` as sole producer
  cache.
- Preserve explicit trace identity precedence, runtime-local active-span
  fallback, fixed table schemas, managed identity columns, and writer/subject
  separation.
- Preserve row-at-a-time observation admission and shutdown as the durability
  barrier.
- Reuse existing SDK journeys and repository-managed servers; do not add a new
  test harness.
- Do not weaken or suppress repository checks and do not hand-edit generated
  artifacts.

## Non-goals

- No new serializer, dependency, config type, queue, cache, producer pool,
  transport, schema authority, lifecycle state, or authorization model.
- No per-language duplication of owner-level concurrent miss/producer tests.
- No synchronous verdict wait, per-observation flush, atomic multi-row API, run
  registry, observation type, or retention policy.
- No documentation refactor or new documentation enforcement tool.
- No unrelated test expansion beyond the four validated gaps.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-002-4` | Root and nested own symbol keys fail with the stable TypeScript validation error before any native call; valid JSON remains unchanged. |
| `FIND-TASK-002-10` | A complete source audit finds rustdoc on every added/materially changed Rust item, including trait and test-local items, with required error/panic/async sections and no suppression. |
| `FIND-TASK-002-13` | All three existing Eval journeys persist session/media and exact non-null trace/span identity, then prove invalid pair and malformed media add no row. |
| `FIND-TASK-002-14` | All three SDK journeys refuse both fixed-table preflight failures and prove cached no-repeat describe; one existing real-server journey proves stable stale-fingerprint refusal. |

## Focused and broader proof

Run the exact TypeScript serializer tests, every added Rust journey/test by its
exact repository-approved `mise exec -- cargo nextest run --locked ... -E
'test(=...)'` command, and the existing Python/TypeScript integration tasks that
own their language runtimes. Record exact test names and results in TASK-002.

Then run the established capability closure:

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

Rerun the focused fixed-size-binary regression and the focused lifecycle,
concurrent-describe, audit-timeout, and denied-describe tests recorded by the
original task so closed adjacent boundaries remain proven.
