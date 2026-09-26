# TASK-002 round 3 — Domain review: public contracts and SDK projections

## Subject and reviewed boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verified-change-contract`
- Immutable subject: base `c8bb490ad814c0c7770cac33ed7779897ff776e4` to candidate `04f73570397d5123eb767abafa60d016c37de1db`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md`
- Prior review inputs: `review/TASK-002-r1/` and `review/TASK-002-r2/`, including both remediation tasks and their validated ledgers
- Reviewed boundary: the Rust-owned scoped observation API; canonical Drift/Eval records and projections; Python/PyO3 and TypeScript/N-API authoring boundaries; active and explicit trace identity; structured errors; generated stubs/declarations; and the three real SDK journeys.

The candidate was `04f73570397d5123eb767abafa60d016c37de1db` at review start and remained that commit after inspection. `.codegraph/` is absent, so source discovery used `rg`, immutable diff inspection, and direct source reads.

## Authority and source coverage

| Boundary | Governing authority | Source and evidence inspected | Result |
|---|---|---|---|
| State-owned writer and immutable Card-scoped invocation | REQ-123, REQ-125, REQ-126, REQ-133; `run_api.md`; observation-identity doctrine | `wyrd-client/src/{state,observe}`; Python state/observe wrappers; TypeScript `WyrdState`, `Run`, `Observe`, and N-API wrapper; all three journeys | PASS |
| Drift accepted inputs and tall fixed projection | REQ-124/125; `run_api.md`; `table_schema.md` | `observe/drift.rs`; `DriftRecordObservation`; Python/TypeScript serializers; projection/unit tests and journey readback | PASS except the TypeScript strict-JSON gap in `DC-R3-001` |
| Eval canonical record, optional identity, and fixed row | REQ-124/125/129; AC-026; telemetry guidance | `observe/eval.rs`; `EvalRecordObservation`; Eval `MediaRef`; Python/PyO3 and TypeScript/N-API boundaries; fixed-binary queue path; real journey readback | PASS |
| Generic explicit-table records | REQ-128; `run_api.md`; Bifrost client pattern | `Observe::record`; `Bifrost::writer_table`; Python/TypeScript wrappers; two-table journey readback and refusal paths | PASS except the same TypeScript input-loss path in `DC-R3-001` |
| Stable cross-language errors | `AGENTS.md` §§4, 8-9; `languages/errors.md`; Python and TypeScript guides | derive-backed error catalog; PyO3 mapper; N-API envelope; wrapper assertions by code | PASS |
| Generated public surfaces | `AGENTS.md` §8 and generated-artifact rule; Python/TypeScript guides | Python package exports/stubs; generated N-API declarations; hand-authored TS wrapper types; recorded `codegen:check`, `py:typecheck`, `ts:typecheck`, and `ts:napi:check` | PASS |
| Required user journeys | `AGENTS.md` §11; `testing-workflows.md`; AC-025/026 | Rust, Python, and TypeScript client → server → client journeys, plus the real stale-writer server journey | PASS |

Applicable authority read for this slice includes `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `architecture/bifrost-design.md`, the approved spec and locked logic references, and the focused contract, PyO3, Python, TypeScript, error, testing, telemetry, and evaluation references.

## Proposed finding

### `DC-R3-001` — INCORRECT — TypeScript still silently drops own properties from accepted arrays and plain objects

- **Violated obligation:** REQ-124 and the locked TypeScript contract require unsupported values and anything `JSON.stringify` would silently omit or coerce to fail before queue admission. This is incomplete behavioral closure of prior `FIND-TASK-002-4`, whose shared `strictJson` boundary is used by Drift, Eval, generic records, and media.
- **Exact location:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1094-1135`, especially the array branch at lines 1116-1119 and `Object.entries` traversal at lines 1125-1130; missing cases in `sdks/wyrd-sdk-ts/wyrd/tests/unit/observe.test.ts:134-178`.
- **Evidence:** the candidate rejects symbol keys only in the non-array branch and traverses plain objects only through enumerable string entries. A direct probe through the built candidate's public `Observe.eval` with a fake native handle admitted an array carrying both `array.extra = 2` and `array[Symbol("secret")] = 3` as `{"array":[1]}`, and admitted a plain object with non-enumerable own `hidden = 2` as `{"visible":1}`; both calls reached native after silently discarding caller-supplied own data. The current table-driven test covers symbol keys on plain objects, but not array-owned symbol/non-index keys or non-enumerable string keys.
- **Observable consequence:** TypeScript callers can enqueue Eval evidence or generic rows that differ from the object they supplied, while the SDK claims that silent JSON omission is rejected; the same boundary can silently alter Drift input before Rust validates it.
- **Required testable correction:** keep the existing `strictJson` owner and add no serializer or dependency. Inspect own keys before traversal: for a plain object, reject symbol keys and non-enumerable own string keys; for an array, reject symbol keys and own string keys other than `length` and its canonical element indices. Preserve the existing value, cycle, prototype, and safe-number checks.
- **Focused closure proof:** extend the existing refused-input table with root/nested non-enumerable plain-object properties and array-owned symbol/non-index properties; exercise Drift, Eval, and record, assert `WYRD_SPEC_400_VALIDATION`, and assert zero native calls. Run that exact Vitest target, `mise run ts:test:unit`, and `mise run ts:typecheck`.

## Prior-finding closure in this domain

| Prior finding | Round-3 status | Evidence |
|---|---|---|
| `FIND-TASK-002-1` configured Rust startup | CLOSED | `start_bifrost_with_config` exposes the existing client/table/`QueueConfig` construction path and the Rust SDK journey compiles it. |
| `FIND-TASK-002-2` exact fixed schemas | CLOSED | Startup compares the complete ordered authored field sequence including type and nullability. |
| `FIND-TASK-002-3` Python key coercion | CLOSED | Mapping/dataclass reductions validate top-level string keys before the required stdlib dump; the direct Pydantic JSON path is preserved. |
| `FIND-TASK-002-4` TypeScript strict JSON | **PARTIAL / OPEN** | Plain-object symbol keys are now rejected, but other own data that JSON drops remains reachable; see `DC-R3-001`. |
| `FIND-TASK-002-5` foreign active spans | CLOSED | Python reads its runtime span; TypeScript reads the installed application OTel API; both explicit precedence and valid active identity are covered, and the TypeScript journey crosses N-API and fixed-binary persisted readback. |
| `FIND-TASK-002-6` single first describe | CLOSED | The shared owner gate/cache is unchanged and the real journeys prove repeated dynamic writes reuse the lookup. |
| `FIND-TASK-002-7` terminal shutdown | CLOSED | Successful shutdown is terminal, including never-started and start-race cases. |
| `FIND-TASK-002-8` ambiguous shutdown retry | CLOSED | State-level proof retries the same writer/batch and then reaches closed state. |
| `FIND-TASK-002-10` rustdoc | CLOSED for this domain slice | The cumulative Rust contract and SDK projection items inspected carry intent-bearing rustdoc and required error/panic sections. |
| `FIND-TASK-002-11` imports/signatures | CLOSED for this domain slice | Reviewed additions retain module-scope imports and bare signature types. |
| `FIND-TASK-002-12` real describe refusals | CLOSED | The language journeys exercise real missing-table refusal; the separate real server path proves denied describe and audit behavior. |
| `FIND-TASK-002-13` Eval journey completeness | CLOSED | Rust, Python, and TypeScript persist and read the required session/media/trace data and prove invalid trace-pair and media refusal without extra rows. |
| `FIND-TASK-002-14` startup/cache/fingerprint journey proof | CLOSED | The three SDK journeys fail each fixed-table preflight and prove cached reuse; the public Rust server journey proves stale flush/register fingerprint refusal and no row landing. |

SQL audit semantics, persistent table durability, tenancy enforcement, and Oracle lease-test correctness are outside this assigned contract slice and remain with their domain reviewers.

## Verification limits

- I inspected the complete cumulative diff and current source, not only the R2 fix commits, and reproduced `DC-R3-001` through the built public TypeScript `Observe` wrapper with a fake native boundary.
- The task records green focused tests and all capability lanes through `4fc251ce`; the later `1f00ce91` full Bifrost integration lane and focused `e6adf997` Oracle tests do not alter this SDK contract finding. I did not rerun broad Cargo/Postgres/Python/TypeScript suites.
- Green existing tests cannot close `DC-R3-001` because their refused-value table lacks these reachable own-property shapes.

## Overall result

**FAIL.** The R2 remediation closes the real SDK journey, trace/media, generated-surface, and fixed-table proof gaps, but TypeScript still violates REQ-124 by silently discarding reachable own properties before the native boundary. The correction is bounded to the existing serializer and one focused test table; no specification revision, new dependency, or new abstraction is required.
