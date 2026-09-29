# TASK-002 R4 data and durability domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verified-change-contract`
- Base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Candidate: `b56560e511918efbdd84d8756b13100fc381eda0`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md`
- Prior review inputs: `review/TASK-002-r1/`, `review/TASK-002-r2/`, and `review/TASK-002-r3/`, including their verdicts, validated ledgers, and remediation tasks
- Reviewed boundary: fixed verification schemas and layouts, JSON-to-Arrow conversion, fixed-width Eval identities, schema description/cache use, fingerprint fencing, stored-row readback, shutdown durability, and the R3 TypeScript serialization correction where it determines the bytes admitted to Bifrost.

`HEAD` resolved to the candidate before inspection and immediately before this
report was written. No candidate source was modified; this report is the
reviewer's only write. The checkout has no `.codegraph/` directory, so source
inspection used `rg`, direct reads, immutable diffs, and a public built-SDK
probe.

AI co-author trailers are explicitly allowed for this review and were not
treated as a finding.

## Authority and source coverage

| Boundary | Authority and inspected source | Result |
|---|---|---|
| Exact task contract and prior closure | Approved spec revision 32, especially REQ-124, REQ-128, REQ-129, REQ-132, AC-025, and AC-026; TASK-002; `architecture/logic/{table_schema,run_api}.md`; R1-R3 validations and remediation tasks | FAIL — one reachable serialization-to-persistence gap remains |
| Bifrost identity, schemas, acknowledgement, recovery, and fingerprint behavior | `architecture/bifrost-design.md`; `architecture/references/domain/{olap-serving,arrow-analytical-interop,analytical-operations-reliability}.md`; `architecture/operations/reliability-and-recovery.md`; five table modules and their catalog contract test | PASS |
| Queue/Arrow conversion | `crates/shared/wyrd-queue/src/batch_builder.rs`, including generic `FixedSizeBinary` decoding and its three focused tests | PASS |
| Client schema ownership and routing | `crates/shared/wyrd-client/src/bifrost/facade.rs`; `observe/{lifecycle,drift,eval,mod}.rs`; `observe/tests.rs`; `pg_bifrost_e2e.rs` | PASS |
| Persisted cross-language evidence | Rust, Python, and TypeScript observe journeys and their R2 evidence for session/media, fixed-width trace/span readback, describe caching, startup refusal, and invalid-input refusal | PASS, subject to the TypeScript gap below |
| R3 data-affecting remediation | `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1080-1152` and `tests/unit/observe.test.ts:134-195`; R3 commits `60f34518`, `9d0cbf77`, and `d6231892` | FAIL — own accessor values are read twice |

## Boundary results

| Concern | Evidence | Result |
|---|---|---|
| Five fixed verification tables | Drift observations, Eval observations, common verification results, Drift result features, and Eval result items still match `table_schema.md` in authored field order, type, nullability, daily partitioning, table-specific Bloom columns, managed Bloom floor, and sensitive payload classification. The exact `tables::tests::verification_tables_match_their_approved_schemas` check passed at the candidate. | PASS |
| Managed identity split | All five tables retain `CorrelationPolicy::Observation`; payload schemas do not author managed `run_id`, `card_uid`, publisher, request, batch, ordinal, ingest, event-time, or tenant columns. Observation projections supply the scoped invocation and subject only through Bifrost correlation. | PASS |
| Fixed-width identity encoding | `decode_lower_hex` accepts exact lowercase hex for the declared byte width and refuses wrong-width, uppercase, prefixed, non-string, and malformed values before Arrow admission. The three focused queue tests passed at the candidate, including IPC readback and managed correlation. | PASS |
| Fixed-table compatibility and cache | Startup describes and fully validates both authored fixed schemas before publishing `Started`; `Bifrost::writer_table` rechecks its owner cache under the existing miss gate and reuses the existing `WriterPool`. R2 journeys cover refused startup and cached reuse. | PASS |
| Stale-schema durability fence | The real Postgres journey flushes a stale writer and attempts stale registration, receives `WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH` for both, and reads no stale row. | PASS |
| Eval persistence | Each SDK journey reads back session/media and fixed-width trace/span identities and proves invalid pairs/media add no row; shutdown remains the explicit client drain barrier rather than a per-observation durability acknowledgement. | PASS |
| R3 hidden/symbol/non-index correction | `Reflect.ownKeys` now rejects non-enumerable and symbol keys on plain objects and symbol/non-index keys on arrays before native admission; the new table rows exercise root and nested forms through Drift, Eval, and generic record. | PASS for the enumerated R3 cases |
| Exact TypeScript value preservation | `strictJson` validates an enumerable property through `Object.entries` and later calls `JSON.stringify` on the original object, so an accessor can return one value during validation and another during serialization. A public `Observe.eval` probe used an enumerable `answer` getter that returned `"yes"` on its first read and `undefined` on its second; the candidate read it twice and invoked native successfully with `{}`. | FAIL (`DATA-R4-001`) |

## Prior-finding closure

| Finding | Data-boundary result |
|---|---|
| `FIND-TASK-002-2`, `-6`, `-8`, `-9`, `-12`, `-13`, `-14` | CLOSED — their schema, cache, retry, audit, refusal, persisted Eval, and real-boundary proofs remain present and unchanged by R3. |
| `FIND-TASK-002-4` | The R3 enumerated hidden/symbol/non-index cases are corrected, but the same stable finding's broader no-silent-omission obligation is not closed because `strictJson` validates and serializes accessor-backed own properties in separate reads. |
| `FIND-TASK-002-11`, `-16`, `-17` | Outside this domain's primary scope; their R3 changes do not alter persistent schemas, Arrow encoding, fingerprint authority, or durability. |

## Proposed finding

### `DATA-R4-001` — TypeScript can validate one accessor value and persist another

- **Classification:** INCORRECT
- **Violated obligation:** REQ-124 and `run_api.md` require silent JavaScript omission or coercion to fail before queue admission, and the retained `FIND-TASK-002-4` outcome requires the JSON delivered to Rust to preserve the caller's accepted own data.
- **Exact location:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1100-1151`, specifically the first accessor read through `Object.entries` at lines 1144-1145 followed by the second read through `JSON.stringify(value)` at line 1151; the refused-input coverage at `sdks/wyrd-sdk-ts/wyrd/tests/unit/observe.test.ts:134-188` has no accessor-backed case.
- **Reachability and evidence:** `Observe.drift`, `Observe.eval`, and `Observe.record` all call this one serializer. Against the built candidate, an ordinary object with an enumerable own getter returned `"yes"` when `Object.entries` validated it and `undefined` when `JSON.stringify` immediately read it again; `Observe.eval` made one native call with `{}` and no error (`reads: 2`). No proxy, native mutation, concurrency, or server fault was involved.
- **Observable consequence:** a caller can receive successful admission while the row/context sent toward the queue silently omits or changes an own property that already passed validation, so persisted evidence can differ from the accepted TypeScript input.
- **Required testable correction:** keep `strictJson` as the single boundary owner and ensure every accepted property value is read once into the JSON-compatible value that is ultimately stringified, rather than validating and then re-reading the caller's object. Preserve stable enumerable accessor support, current key/prototype/cycle/scalar checks, and the existing error code; do not add a serializer or dependency.
- **Focused closure proof:** add root and nested enumerable accessors whose second read would return an omitted or invalid value, run them through Drift, Eval, and generic record, and prove either exact first-read JSON reaches native once or a documented validation refusal occurs before native; also assert each getter is read once. Run the exact existing `observe.test.ts` target plus `mise run ts:test:unit` and `mise run ts:typecheck`.

## Verification limits

This review reran, at the exact candidate, all three named `wyrd-queue`
batch-builder checks and the exact Vala five-table schema contract; all four
passed. `git diff --check` over the cumulative range passed. The public
TypeScript probe above used the already-built candidate package and made no
source change.

The review did not rerun Postgres or the three full SDK journeys. The task
records `verify:bifrost` as 9/9 at `d6231892`, plus the TypeScript unit,
typecheck, and integration lanes, formatting, lints, and exact queue/client
tests. The only commits after that functional head are the evidence commit
`b56560e5`; static diff inspection confirms no data-path source changed after
the recorded aggregate.

TASK-002 defines but does not yet publish Verifier result/detail rows;
same-event-time multi-table result persistence and result/detail partial
acknowledgement remain later-task obligations and are not findings here.

## Overall result

**FAIL** — the Bifrost schemas, Arrow conversion, cache/fingerprint fences,
readback, and shutdown durability satisfy TASK-002, but the shared TypeScript
serializer can still admit JSON that silently differs from the own value it
validated, leaving the persistent observation boundary incorrect.
