# TASK-002 R5 Retry 1 — Task Implementation Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verified-change-contract`
- Original base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Cumulative candidate: `935cc6324d213414402a1d73d6eeec475965fbcb`
- Range: `c8bb490ad814c0c7770cac33ed7779897ff776e4..935cc6324d213414402a1d73d6eeec475965fbcb`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md`
- Logic authorities: `changes/active/verified-change-contract/architecture/logic/run_api.md` and `table_schema.md`
- Prior review/remediation inputs: `review/TASK-002-r1/` through `review/TASK-002-r4/`

`HEAD` was the candidate before and after inspection. The unrelated untracked
`review/TASK-002-r5/` directory was not treated as candidate source. Current
user authority allows the existing AI co-author trailers, so commit metadata is
not a finding.

## Scope inspected

I inspected the complete cumulative diff, the original task and locked logic
references, all prior stable findings and their remediation, the resulting
shared Rust owners, fixed-table catalog and queue conversion, Python and
TypeScript projections, real SDK journeys, adjacent SQL/test-harness fixes,
and the R4 TypeScript implementation and focused tests. The implementation
summary was not used as acceptance evidence.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| One state-owned Bifrost lifetime; configured/default startup preflights both fixed tables, rejects repeated startup, and closes terminally after successful shutdown | `wyrd-client/src/state.rs`, `observe/lifecycle.rs`, and the existing `Bifrost::connect_with_config` path retain one owner and the corrected fenced phases | `observe::tests` lifecycle cases; Rust/Python/TypeScript journeys | PASS |
| Ambiguous shutdown remains retryable on the same handle and graceful shutdown drains all producers | Existing Bifrost/producer retry path is retained under `WyrdState::shutdown` | `ambiguous_shutdown_retries_the_same_batch_on_the_same_state`; SDK journey shutdown/readback | PASS |
| One invocation mints one ID and immutable root/Card-scoped sibling views preserve the exact UID-bearing subject | `observe/run.rs` and `observe/mod.rs` keep immutable views over shared invocation state and hydrated alias lookup | Shared observe tests plus all three real SDK journeys | PASS |
| Unknown or out-of-graph aliases fail locally without network IO | Hydrated state index lookup precedes observation IO | Shared observe tests and journey assertions | PASS |
| Drift accepts the approved language inputs, validates the existing feature map, and projects one tall fixed row per feature | Python/TypeScript boundaries converge on shared Rust `observe/drift.rs`; no second feature-map contract exists | Shared projection tests, SDK boundary tests, and three journeys | PASS |
| Drift rejects null/nested/nonfinite/unsafe numeric/invalid-name input before admission and preserves numeric categorical strings | `strictJson`, Python strict JSON, existing `FeatureName`/`FeatureValue`, and Rust projection perform the boundary checks | Focused SDK unit tests and shared Rust tests | PASS |
| Eval constructs the existing canonical observation with session, media, trace/span precedence, record/time identity, and no duplicate run/Verifier identity | `observe/eval.rs`, Python/TypeScript options, and canonical `wyrd-spec` record retain the approved split | AC-026 coverage in Rust/Python/TypeScript journeys reads persisted values back | PASS |
| Invalid trace pairs and malformed media fail before queue admission | Shared Eval validation and foreign-runtime projections return the stable validation error | Each real SDK journey proves no added row for both negative cases | PASS |
| TypeScript accepts only exactly representable JSON input and does not silently omit or coerce caller data | `strictJson` at `index.ts:1103-1158` rejects unsupported keys/values and builds the exact recursively validated snapshot that it stringifies | `observe.test.ts` refused-input, ordinary nested JSON, and one-read accessor cases | PASS |
| Getter-backed TypeScript object fields and array elements are read once and native receives the accepted first value | The R4 traversal returns a new snapshot from the same single property/element reads rather than stringifying the original object | `observe.test.ts:191-230` covers root, nested, array, nested-array, and unsupported-first-value paths through Drift, Eval, and record; focused rerun passed 10/10 | PASS |
| Eval media descriptors preserve the closed shape and refuse symbol, hidden, or undeclared data before native admission | `mediaJson` at `index.ts:1191-1220` checks original own keys, reads declared fields once, renames only `mediaType`, and delegates the snapshot to `strictJson` | `observe.test.ts:233-259` proves the three refusals, zero native calls, and one-read accepted projection | PASS |
| Generic `FixedSizeBinary(16)` and `(8)` columns decode exact lowercase hex, reject malformed/wrong-width values, and round-trip bytes | Generic conversion remains in `wyrd-queue/src/batch_builder.rs`, independent of Eval | Three focused `batch_builder_tests` recorded as passing | PASS |
| Dynamic records use an explicit `vala.datasets` table, describe once under concurrent first use, cache the schema, and never mutate an active table | Existing Bifrost facade cache plus its owner-local miss gate and `WriterPool::insert` remain authoritative | Concurrent describe unit test; SDK journeys write twice and observe one describe | PASS |
| Reserved, unknown, and unauthorized destinations fail before admission; stale fingerprints are fenced | Local namespace guard, server describe authorization, and existing Gate/Scribe fingerprint fence are preserved | Three real journeys, audited denied-describe journey, and stale-writer server journey | PASS |
| All three SDK journeys cross client, queue, IPC, Gate, Scribe, shutdown, and query readback with Model/Agent scope switching and two generic tables | Rust `observe_run.rs`, Python `test_observe_journey.py`, and TypeScript `observe-run.test.ts` use real `WyrdTestServer`/Postgres paths | Previously recorded `verify:bifrost` 9/9 runs and language integration lanes | PASS |
| The five fixed verification tables exactly match approved order, nullability, physical types, partitions, Blooms, sensitivity, and identity split | Vala built-in table modules and exact startup schema comparison implement `table_schema.md`; managed writer identity stays outside user rows | Exact catalog/schema tests and successful real starts/writes/readbacks | PASS |
| Startup fails when either fixed table cannot be described or has an incompatible schema | Fixed-table preflight validates the complete ordered user-field sequence before lifecycle completion | Incompatible-schema unit cases and per-table real journey fault/restore loops | PASS |
| Observation calls enqueue through the existing bounded queue without per-record describe/flush, synchronous ACK, or verdict wait | Shared observe projections call the existing explicit-table Bifrost insert/producer path; only first dynamic-table use may describe | Shared tests and SDK journeys | PASS |
| Shared client and queue plumbing remain Verifier-kind agnostic | Bifrost and queue APIs operate on table/schema/row/correlation, while Drift/Eval projection stays at its owner boundary | Complete diff inspection | PASS |
| Public Rust, Python, and TypeScript APIs match `run_api.md`, including configured Rust startup and readonly closed TypeScript option shapes | Shared client API, PyO3/Python exports/stubs, and TypeScript declarations/projectors match the locked examples | Rust compile/journey use, Python typecheck, TypeScript typecheck, and codegen evidence | PASS |
| New and materially changed Rust items satisfy documentation/import structure requirements | Prior remediations document cumulative changed declarations and use module-top imports/bare declaration types | Workspace lints, formatting, and Clippy-allow audit recorded clean | PASS |
| Audit lock timeout and Forge clock-domain fixes forced by verification preserve honest transactions and database-clock eligibility | `vala-sql` propagates the lock timeout and stamps eligibility from `statement_timestamp()` | Focused Postgres audit/Forge tests and prior full lanes | PASS |
| Test-only server hooks cannot change production audit behavior | Audit-publication controls and describe probes remain behind `test-support` and are consumed only by test SDK projections | Feature-split/lint checks and integration journeys | PASS |
| No second queue, producer pool, transport, schema authority/cache, lifecycle vocabulary, run registry, observation type, authorization model, flush, ACK, or retention system was introduced | Complete cumulative diff retains the named existing owners and uses one serializer at the TypeScript boundary | Source inspection and boundary checks | PASS |
| No generalized validation framework, new dependency, public type/error, or duplicated Rust validation was added for R4 | R4 changes only `index.ts` and its existing unit test, reusing `strictJson`, `WyrdError`, and standard JS reflection | R4 diff inspection; focused unit rerun | PASS |

## Prior-finding closure

| Stable finding | Status at candidate |
|---|---|
| `FIND-TASK-002-1` | CLOSED — configured Rust startup remains public and exercised. |
| `FIND-TASK-002-2` | CLOSED — fixed schemas are compared exactly and in order. |
| `FIND-TASK-002-3` | CLOSED — Python rejects non-string mapping/dataclass keys before JSON coercion. |
| `FIND-TASK-002-4` | CLOSED — R4 now serializes the one-read validated snapshot and validates original media descriptor keys before projection. |
| `FIND-TASK-002-5` | CLOSED — explicit IDs take precedence and Python/Node active spans are projected at their runtime boundaries. |
| `FIND-TASK-002-6` | CLOSED — racing cache misses converge through the existing Bifrost owner gate. |
| `FIND-TASK-002-7` | CLOSED — successful shutdown is terminal across start races. |
| `FIND-TASK-002-8` | CLOSED — ambiguous drain retry preserves handle and batch identity. |
| `FIND-TASK-002-9` | CLOSED — audit lock timeout is propagated from the caller-owned transaction. |
| `FIND-TASK-002-10` | CLOSED — cumulative changed Rust declarations are documented. |
| `FIND-TASK-002-11` | CLOSED — cited Rust declarations use module-top imports and bare names. |
| `FIND-TASK-002-12` | CLOSED — real unknown and denied describes fail before admission with audit evidence. |
| `FIND-TASK-002-13` | CLOSED — all SDK Eval journeys prove persisted session/media/trace identity and required refusals. |
| `FIND-TASK-002-14` | CLOSED — SDK journeys prove fixed-table startup refusal/cache reuse and the shared server journey proves stale-fingerprint fencing. |
| `FIND-TASK-002-16` | CLOSED — Eval media/options are readonly closed object type aliases. |
| `FIND-TASK-002-17` | CLOSED — evidence names the implemented `WYRD_SPEC_400_VALIDATION` code. |

No prior finding remains open, and this review proposes no new finding.

## Proposed findings

None.

## Verification limits

The focused R4 test was independently rerun at this candidate with
`mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run
tests/unit/observe.test.ts` and passed all 10 tests. The reported R4
`ts:test:unit` (22), `ts:test:integration` (18), `ts:typecheck`, `fmt`, `lints`,
and `git diff --check` results are consistent with the inspected source.

The latest `verify:bifrost` invocation had not completed when this review
started, so it is not credited as candidate-level evidence. Earlier cumulative
candidates passed its nine lanes after the lifecycle, schema, authorization,
SDK journey, and Oracle fixes, and R4 changes only the TypeScript serialization
boundary plus focused unit coverage. This is a verification limitation, not an
observed implementation failure and not a material acceptance gap for the
bounded correction.

## Overall result

**PASS**

The cumulative candidate satisfies TASK-002's implementation obligations and
non-goals, closes the last stable finding at the existing TypeScript boundary,
and introduces no task-relevant regression or unnecessary replacement
mechanism.
