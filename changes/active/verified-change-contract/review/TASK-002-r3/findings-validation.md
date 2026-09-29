# TASK-002 R3 structured Ponytail validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verified-change-contract`
- Base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Candidate: `04f73570397d5123eb767abafa60d016c37de1db`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md`
- Prior review authority: `review/TASK-002-r1/` and `review/TASK-002-r2/`, including both verdicts, validations, and remediation tasks

The complete cumulative diff and all six Wave 1 reports were inspected. The
repository has no `.codegraph/` directory, so caller tracing used `rg`, direct
source reads, immutable diff inspection, and a public built-SDK probe. `HEAD`
resolved to the candidate before validation and again after this report was
written.

## Wave 1 proposal validation

| Proposal | Decision | Independent validation and minimum correction |
|---|---|---|
| `TASKREV-R3-001` | **REJECTED** | The user explicitly authorized AI co-author trailers for this task review, so the repository default does not create a remediation finding. |
| `STD-R3-1` | **REVISED**, reopening `FIND-TASK-002-11` | The rule is explicit, and the cited additions are reachable: `credential_registered_service` is called by the Rust journey and both foreign-runtime test bindings; `reserve_loopback_addr` is called by server and cluster startup; its full helper chain is `reserve_loopback_addr` → `reservation_band` / `bound_loopback_addr`; and the two cutoff constants are consumed by the blocked-renewal test. The candidate adds `service_account_by_card_ref` to a `use` block after module items and adds qualified `Range`, `OnceLock`, `AtomicU32`, `SocketAddr`, and `Duration` types in new signatures/statics/constants. Reuse the existing top import groups and bare names; do not reformat unrelated pre-existing qualified body expressions or create a style check. |
| `STD-R3-2` | **CONFIRMED** as new `FIND-TASK-002-16` | `EvalMediaRef` is consumed only by `EvalOptions` and `mediaJson`; `EvalOptions` is consumed by `Observe.eval`; neither declaration is merged, extended, or otherwise requires interface openness. The applicable TypeScript guide explicitly requires types over interfaces unless declaration merging is needed. Converting these two declarations to readonly object type aliases changes no runtime or wire behavior and is the complete correction. |
| `STD-R3-3` | **CONFIRMED** as new `FIND-TASK-002-17` | The committed evidence row says `WYRD_SDK_400_INVALID_OBSERVATION`, but the full `invalidObservationInput` body constructs `WYRD_SPEC_400_VALIDATION` and the focused test asserts that exact code. The implementation follows the pre-existing validation path required by R2; only the evidence is false. Correct the one evidence cell rather than inventing a new error or changing working code. |
| `DC-R3-001` | **CONFIRMED**, reopening `FIND-TASK-002-4` | Full caller tracing found exactly four uses of `strictJson`: Drift features, Eval context, generic records, and the normalized media array. The public `Observe.eval` probe admitted an array carrying `extra` and a symbol key as `[1]`, and admitted `{ visible: 1 }` with a non-enumerable own `hidden` property as `{"visible":1}`; both reached native. The original requirement and R1 correction require values that JavaScript silently omits or coerces to fail before native admission, and the R2 intended outcome says no own input property may disappear. Keep the one serializer and reject own plain-object keys that JSON would omit plus array-owned symbol/non-index keys before traversal; add no serializer, dependency, or Rust validation. |

The task and security reviewers' closure of `FIND-TASK-002-4` is rejected
because it considers only the two newly added symbol-key tests, not the
reachable sibling omission paths through the same serializer. Their closure of
`FIND-TASK-002-11` is likewise superseded by the exact changed-line evidence in
the standards report. No authority conflict remains.

## Deduplicated final finding ledger

### `FIND-TASK-002-4` — TypeScript still silently discards own observation data

- **Wave 1 source:** `DC-R3-001`
- **Status:** CONFIRMED / REOPENED
- **Classification:** INCORRECT
- **Violated obligation:** REQ-124, `run_api.md`, and the approved R1/R2 correction require unsupported values and JavaScript omission or coercion to fail before native queue admission; R2's intended outcome explicitly prohibits silently discarding any own input property.
- **Exact location:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1094-1135`, especially the array traversal at lines 1116-1119 and `Object.entries` traversal at lines 1125-1130; missing cases in `sdks/wyrd-sdk-ts/wyrd/tests/unit/observe.test.ts:134-181`.
- **Evidence:** Arrays are traversed only by numeric length and plain objects only by enumerable string entries. A public built-SDK `Observe.eval` probe sent `{"array":[1]}` after the supplied array also owned `extra = 2` and a symbol key, and sent `{"visible":1}` after the supplied plain object also owned a non-enumerable `hidden = 2`; both native calls occurred without error. Drift, Eval, and record all route through this owner.
- **Observable consequence:** Eval evidence and generic rows can differ from the caller's supplied object while reporting successful admission; the same shared boundary can pass a silently altered Drift value to native validation.
- **Decision-complete correction:** Keep `strictJson` as the sole TypeScript serializer. Before value traversal, reject a plain object's own symbol keys and non-enumerable string keys; for an array, reject own symbol keys and string keys other than `length` and canonical in-range element indices. Preserve current prototype, cycle, scalar, safe-number, and optional-media behavior. Do not add a dependency or duplicate the check in Rust.
- **Focused closure proof:** Extend the existing refused-input table with root and nested non-enumerable plain-object properties and array-owned symbol/non-index properties; run each through Drift, Eval, and record, assert `WYRD_SPEC_400_VALIDATION`, and assert zero native calls. Run the exact Vitest target, `mise run ts:test:unit`, and `mise run ts:typecheck`.

### `FIND-TASK-002-11` — Changed Rust items still violate import and bare-type rules

- **Wave 1 source:** `STD-R3-1`
- **Status:** REVISED / REOPENED
- **Classification:** VIOLATION
- **Violated obligation:** `architecture/agent-rules.md` requires module-top imports and imported bare types in signatures, fields, bounds, and related item declarations; `AGENTS.md` applies repository style to new and materially modified Rust.
- **Exact location:** `crates/wyrd/wyrd-testing/src/server.rs:144-160,4455-4512` and `crates/wyrd/wyrd-server/tests/pg_router_smoke.rs:1106-1115`.
- **Evidence:** The candidate adds `service_account_by_card_ref` to a late import block and spells new `Range`, `OnceLock`, `AtomicU32`, `SocketAddr`, and `Duration` declaration types with qualified paths. The full helper and consumer trace confirms these are live task/harness paths, not dormant examples.
- **Observable consequence:** The changed modules hide newly introduced dependencies outside the module's declared import manifest and leave changed item declarations in an explicitly prohibited form.
- **Decision-complete correction:** Add only the newly needed symbols to the existing top import groups, remove `service_account_by_card_ref` from the late block, and use the imported bare names in the changed declarations. Preserve the helper algorithms, Oracle timing, and unrelated pre-existing code; add no enforcement script.
- **Focused closure proof:** Inspect the corrected changed declarations, then run `mise run fmt`, `mise run lints`, and `mise run check:clippy-allow-audit`.

### `FIND-TASK-002-16` — New TypeScript data shapes use open interfaces without a merging need

- **Wave 1 source:** `STD-R3-2`
- **Status:** CONFIRMED
- **Classification:** VIOLATION
- **Violated obligation:** `architecture/references/languages/typescript-guide.md` requires types over interfaces unless declaration merging is required.
- **Exact location:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1044-1066` (`EvalMediaRef` and `EvalOptions`).
- **Evidence:** Repository-wide use tracing finds no declaration merge or extension: `EvalMediaRef` feeds `EvalOptions`/`mediaJson`, and `EvalOptions` feeds only `Observe.eval`. Both are closed public observation input shapes introduced by this task.
- **Observable consequence:** Consumers can declaration-merge fields into API shapes intended to describe the exact Eval authoring contract, contrary to the SDK's required declaration style.
- **Decision-complete correction:** Replace only these two interfaces with exported readonly object type aliases preserving every field, optional marker, union, and documentation. Do not change runtime conversion or generated native declarations.
- **Focused closure proof:** Run `mise run ts:typecheck`, `mise run ts:test:unit`, and `mise run ts:test:integration`.

### `FIND-TASK-002-17` — The committed R2 evidence names the wrong error code

- **Wave 1 source:** `STD-R3-3`
- **Status:** CONFIRMED
- **Classification:** VIOLATION
- **Violated obligation:** `AGENTS.md` completion and planning rules require acceptance and verification evidence to accurately describe what the implementation and test prove.
- **Exact location:** `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md:397`.
- **Evidence:** The evidence row claims `WYRD_SDK_400_INVALID_OBSERVATION`; `invalidObservationInput` and `observe.test.ts` both use and assert `WYRD_SPEC_400_VALIDATION`.
- **Observable consequence:** A maintainer following the immutable task record is given a stable public failure contract that the SDK does not implement.
- **Decision-complete correction:** Change only that evidence cell to `WYRD_SPEC_400_VALIDATION`; preserve the implementation's existing catalog-backed validation path.
- **Focused closure proof:** Confirm the row, helper, and unit assertion name the same code, rerun the exact `observe.test.ts` Vitest target, and run `git diff --check`.

## Prior-finding closure

| Stable finding | R3 validation status |
|---|---|
| `FIND-TASK-002-1` | CLOSED — configured Rust startup remains public and exercised. |
| `FIND-TASK-002-2` | CLOSED — fixed schemas are compared completely and in order. |
| `FIND-TASK-002-3` | CLOSED — Python rejects non-string mapping/dataclass keys before serialization. |
| `FIND-TASK-002-4` | **OPEN / REOPENED** — symbol-keyed plain objects are fixed, but reachable array-owned and non-enumerable own data still disappears. |
| `FIND-TASK-002-5` | CLOSED — foreign runtimes project active spans with explicit precedence. |
| `FIND-TASK-002-6` | CLOSED — first-use describes converge through the existing owner gate/cache. |
| `FIND-TASK-002-7` | CLOSED — successful shutdown is terminal across races. |
| `FIND-TASK-002-8` | CLOSED — ambiguous drain retry retains the same writer and batch. |
| `FIND-TASK-002-9` | CLOSED — audit lock timeout propagates and preserves retry state. |
| `FIND-TASK-002-10` | CLOSED — the cumulative changed-item rustdoc gap is remediated. |
| `FIND-TASK-002-11` | **OPEN / REOPENED** — other newly changed harness/test declarations retain late imports or qualified declaration types. |
| `FIND-TASK-002-12` | CLOSED — real unknown and denied describes fail before admission with audit proof. |
| `FIND-TASK-002-13` | CLOSED — every SDK Eval journey covers authored options, persisted identity, and negative admission. |
| `FIND-TASK-002-14` | CLOSED — the real boundaries cover fixed-table refusal, cached reuse, and stale fingerprint fencing. |

## Overall validation result

**NON-EMPTY VALIDATED LEDGER.** Four bounded corrections remain:
`FIND-TASK-002-4`, `FIND-TASK-002-11`, `FIND-TASK-002-16`, and
`FIND-TASK-002-17`. All reuse existing owners or
repository mechanisms; none requires a new product, public API, architecture,
security, compatibility, concurrency, resource-ownership, or persistent-data
decision. No specification revision is required.
