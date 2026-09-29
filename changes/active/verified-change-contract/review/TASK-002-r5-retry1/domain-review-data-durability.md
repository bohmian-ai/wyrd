# TASK-002 R5 retry 1 data and durability domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verified-change-contract`
- Base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Candidate: `935cc6324d213414402a1d73d6eeec475965fbcb`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md`
- Prior review and remediation inputs: `review/TASK-002-r1/` through `review/TASK-002-r4/`
- Reviewed boundary: fixed verification schemas, generic JSON-to-Arrow conversion, fixed-width identities, Bifrost schema caching and fingerprint fencing, persisted readback, drain durability, and the R4 TypeScript one-read snapshot correction.

`HEAD` resolved to the candidate before inspection and immediately before this
report was written. The checkout has no `.codegraph/` directory, so inspection
used immutable diffs, `rg`, and direct source reads. No candidate source was
modified. AI co-author trailers are allowed and are outside this review.

## Authority and source coverage

| Boundary | Authority and inspected source | Result |
|---|---|---|
| TASK-002 data obligations and R4 closure | Spec revision 32, especially REQ-124, REQ-128, REQ-129, REQ-132, AC-025, and AC-026; original task; `architecture/logic/{run_api,table_schema}.md`; R1-R4 verdicts, ledgers, and remediation tasks | PASS |
| Physical schemas and persistence model | `architecture/bifrost-design.md`; five Vala table definitions and their exact catalog contract test | PASS |
| Queue and Arrow conversion | `crates/shared/wyrd-queue/src/batch_builder.rs`, including all three fixed-size-binary focused tests | PASS |
| Bifrost routing, schema ownership, and durability | `wyrd-client/src/bifrost/{facade,table}.rs`; `observe/{mod,lifecycle,drift,eval,tests}.rs`; `pg_bifrost_e2e.rs` | PASS |
| Cross-language persisted evidence | Rust, Python, and TypeScript observe journeys; recorded R2/R3 evidence for session/media and trace/span readback, startup refusal, schema cache reuse, stale-fingerprint refusal, and invalid-input no-write behavior | PASS |
| R4 TypeScript input-to-persistence correction | Candidate diff in `sdks/wyrd-sdk-ts/wyrd/src/index.ts` and `tests/unit/observe.test.ts`; exact focused Vitest run at the candidate | PASS |

## Boundary results

| Concern | Evidence | Result |
|---|---|---|
| Five fixed tables | The authored schemas, order, nullability, partitions, Bloom columns, managed Bloom floor, and sensitive classifications remain unchanged from the previously accepted definitions and match `table_schema.md`; `tables::tests::verification_tables_match_their_approved_schemas` passed at this candidate. | PASS |
| Writer/subject identity split | All five tables retain `CorrelationPolicy::Observation`; clients author neither managed invocation/subject fields nor publisher, tenant, request, batch, ordinal, ingest, or event-time identity. | PASS |
| Fixed-width Arrow identities | `decode_lower_hex` still accepts exact lowercase hex for `FixedSizeBinary(16)` and `(8)` and refuses malformed, uppercase, prefixed, or wrong-width values before admission; all three focused queue tests passed at this candidate. | PASS |
| Startup, cache, and fingerprint fence | Fixed startup compatibility checks, owner-mediated first-schema cache convergence, shared `WriterPool`, and server-side stale-fingerprint rejection remain unchanged by R4; prior real-boundary journeys prove refused startup, cached reuse, and no stale-row persistence. | PASS |
| Readback and shutdown durability | The three SDK journeys still read exact session/media and trace/span values by subject Card UID and invocation ID, while successful shutdown remains the explicit drain barrier rather than a per-observation Scribe acknowledgement. | PASS |
| One-read TypeScript snapshot | `strictJson` now returns a recursively validated plain snapshot and stringifies that snapshot, so each accepted object property or array element is read once and the accepted value is the value sent to native. The tests cover root/nested object accessors and root/nested array accessors through Drift, Eval, and generic record, assert one read, assert exact native JSON, and refuse a first-read `bigint`. | PASS |
| Closed Eval media projection | `mediaJson` checks every original descriptor's own keys against `id`, `kind`, `uri`, and `mediaType`, rejects undeclared, symbol, and non-enumerable keys before native admission, reads the four allowed fields once, then sends the projected snapshot through the same `strictJson`; focused tests prove refusal and one-read `mediaType` projection. | PASS |
| Scope preservation | R4 changes only the existing TypeScript serializer and media projection plus their unit tests; it adds no serializer, dependency, durable schema, queue, cache, producer pool, transport, lifecycle state, acknowledgement, or authorization model. | PASS |

## R4 caller and persistence tracing

`Observe.drift`, `Observe.eval`, and `Observe.record` still converge on the one
`strictJson` boundary before their native calls. The recursive `check` now
copies each permitted scalar into arrays or `Object.fromEntries`, retains the
existing plain-object, own-key, cycle, finite-number, safe-integer, and
unsupported-value refusals, and calls `JSON.stringify` only on that completed
snapshot. `mediaJson` validates original media keys before its necessary
`mediaType` to `media_type` projection and supplies only ordinary snapshot
objects to `strictJson`. Thus the R4 accessor and pre-projection omission paths
are closed once in their shared owners without changing Rust validation or the
Bifrost persistence contract.

## Prior-finding closure

| Finding | Data/durability status |
|---|---|
| `FIND-TASK-002-4` | CLOSED — the shared TypeScript boundary now stringifies its exact one-read validated snapshot, and original Eval media descriptors cannot lose undeclared, symbol, or hidden own data during projection. |
| `FIND-TASK-002-2`, `-6`, `-8`, `-9`, `-12`, `-13`, `-14` | CLOSED — exact schemas, cache convergence, retry/audit behavior, fixed-width persisted Eval identity, denied describe, and stale-fingerprint real-boundary proofs remain present and are unaffected by R4. |
| Other prior findings | Outside this domain or previously closed; R4 does not reopen a persistent schema, Arrow, routing, or durability boundary. |

## Findings

No material data or durability findings.

## Verification and limits

At candidate `935cc6324d213414402a1d73d6eeec475965fbcb`, this review ran:

- `pnpm exec vitest run tests/unit/observe.test.ts`: 10/10 passed;
- the exact three `wyrd-queue` fixed-size-binary/correlation nextest filters: 3/3 passed;
- `tables::tests::verification_tables_match_their_approved_schemas`: 1/1 passed;
- `mise run ts:test:unit`: exited successfully; and
- cumulative `git diff --check`: clean.

The implementation report additionally records `ts:test:integration` (18),
`ts:typecheck`, `fmt`, and `lints` passing. The full `verify:bifrost` rerun was
still in progress when this review completed, so aggregate confirmation is a
verification limit; it is not evidence of a data defect, and the only R4 source
changes are the TypeScript boundary and its focused tests.

TASK-002 defines table contracts but does not publish Verifier result/detail
rows; same-event-time multi-table result persistence remains a later-task
obligation and is not a finding here.

## Overall result

**PASS** — R4 closes the reachable TypeScript validate-one-value/send-another
path and the pre-validation media-key omission path, while the cumulative
schemas, Arrow conversion, cache/fingerprint fences, readback, and shutdown
durability boundaries continue to satisfy TASK-002.
