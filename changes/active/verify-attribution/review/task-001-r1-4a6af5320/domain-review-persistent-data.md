# Persistent trace data and query review

## Subject and boundary

- Base: `ef4f6b0251e9d11c2712765f6db90545e40d590b`
- Candidate: `4a6af5320ab8c236fbe640ff0dc5ddf544bdcde2`
- Approved authority: `changes/active/verify-attribution/spec.md` revision 3
- Original task: `changes/active/verify-attribution/tasks/TASK-001-distributed-trace-continuity.md`
- Prior finding under direct closure review: `FIND-TASK-001-2`

This pass covers persisted span identity, the one-hop timeline query, row
decoding, deterministic ordering and deduplication, tenant binding, and the
canonical `SpanRecord` projection through Rust, Python, and TypeScript after
the two main merges. It does not review telemetry routing, sampling, the
outbox byte budget, or the Scribe-outage test except where their output becomes
input to the timeline read.

## Authority and source coverage

| Boundary | Authority and source inspected | Result |
| --- | --- | --- |
| Persistent span and tenant identity | `architecture/bifrost-design.md` table/row identity and tenant binding; `architecture/references/domain/olap-serving.md`; `architecture/references/domain/arrow-analytical-interop.md`; `vala.traces.spans` fixture writes in `crates/shared/wyrd-client/tests/integration/pg_bifrost_e2e/trace_timeline.rs` | PASS |
| Timeline closure and query safety | SPEC r3 REQ-008/INV-001; `crates/shared/wyrd-client/src/bifrost/facade.rs::Bifrost::trace_timeline`; the existing authenticated `Bifrost::sql` path | PASS |
| Canonical row decoding | `crates/shared/wyrd-client/src/bifrost/rows.rs::{SPAN_COLUMNS,span_record,event,link}` and `crates/wyrd-spec/src/vala/trace/{span,span_event,span_link,instrumentation_scope,resource}.rs` | PASS |
| Ordering, identity deduplication, first-copy choice | remediation task FIND-2; commit `6464d5c31`; `facade.rs`; strengthened real-server fixture `deduplicates_and_orders_ties_by_trace_and_span` | PASS |
| Rust projections | async `Bifrost::trace_timeline`, blocking facade, canonical `wyrd_spec::vala::trace::SpanRecord`, Rust action-timeline callers | PASS |
| Python projections after generator retirement | AGENTS.md §8; `architecture/references/languages/python-api-and-stubs.md`; native `sdks/wyrd-sdk-python/src/bifrost/mod.rs::trace_timeline`; authored `python/wyrd/bifrost/_client.py`; exports and action-timeline callers | FAIL: `PD-R1-001` |
| TypeScript projections after the new generator | `architecture/references/languages/typescript-guide.md`; native N-API `trace_timeline`; generated `card-types.ts::SpanRecord`; `index.ts::traceTimeline`; timeline support and journey callers | PASS |

## Persistent-query assessment

The query binds the typed trace id as `$1`, reads only the fixed
`vala.traces.spans` table, returns the seed trace plus every span belonging to a
trace with a direct link to the seed, and does not recurse into a second hop.
It runs through the existing authenticated Oracle path, so the physical table
binding remains tenant-qualified; no caller tenant predicate or unbound pool
was added.

`span_record` decodes the stored binary identifiers into the canonical typed
`TraceId` and `SpanId`, reconstructs persisted status, attributes, events, and
links, and supplies the canonical resource/scope envelope required by REQ-008.
No positional schema mapping was introduced. The existing documented limited
projection of resource attributes and scope version predates this remediation
and still returns every field REQ-008 names.

## Prior-finding closure

### `FIND-TASK-001-2` — closed in source

`crates/shared/wyrd-client/src/bifrost/facade.rs:857-875` keeps the required
sort by `(start_time, trace_id, span_id)` and then retains the first row for
each `(trace_id, span_id)` using one local `HashSet`. This removes duplicates
regardless of whether another span sorts between two stored copies, without a
second query, SDK-specific logic, or a new abstraction.

`crates/shared/wyrd-client/tests/integration/pg_bifrost_e2e/trace_timeline.rs:142-176`
writes duplicate identity `0a/01` at offsets 0 and 20, places another span at
offset 10, and asserts that only the earlier copy remains in the total order.
The fixture uses a real `WyrdTestServer`, public Arrow batch write, flush, and
the public timeline read, so it exercises stored rows rather than an in-memory
stand-in.

## Material proposed finding

### `PD-R1-001` — REGRESSION: Python's authored `SpanRecord` graph marks always-emitted canonical fields optional

- Violated obligation: REQ-008 requires the Python SDK to project the same
  canonical `SpanRecord` returned by the Rust owner; AGENTS.md §8 and
  `python-api-and-stubs.md` require authored public typing to match the native
  API with precise types.
- Exact locations:
  - `sdks/wyrd-sdk-python/python/wyrd/bifrost/_client.py:1319-1403`
  - `sdks/wyrd-sdk-python/src/bifrost/mod.rs:789-793`
  - canonical serialization authority at
    `crates/wyrd-spec/src/vala/trace/span.rs:22-70`, plus its resource, scope,
    event, link, and status types
- Evidence: the native binding serializes the canonical Rust records directly
  to plain Python JSON. Rust `#[serde(default)]` affects deserialization but
  does not omit serialization, so `flags`, `trace_state`, `attributes`, all
  dropped counts, `events`, and `links` are always present. The same is true
  of resource/scope/event/link attribute and count fields. The Python
  `_ResourceOptional`, `_InstrumentationScopeOptional`, `_SpanEventOptional`,
  `_SpanLinkOptional`, and `_SpanRecordOptional` bases instead declare those
  keys as possibly absent. Python's single `SpanStatus` also admits impossible
  status shapes, while the Rust contract and generated TypeScript contract are
  the `unset | ok | error` tagged union. The TypeScript generator correctly
  makes every always-emitted field required in
  `sdks/wyrd-sdk-ts/wyrd/src/card-types.ts:6057-6137`.
- Reachability: both public Python `Bifrost.trace_timeline` methods return this
  authored type, and the Python action-timeline journey consumes it. This is
  the direct public projection added for REQ-008, not dormant or test-only
  code.
- Observable consequence: Python editors and static checking cannot rely on
  fields that every successful timeline result guarantees, and the public
  type admits incomplete or invalid records that neither Rust nor the wire can
  produce. Rust and TypeScript therefore expose a stricter canonical contract
  than Python for the same API.
- Smallest correction: keep the post-merge plain-dictionary/`TypedDict`
  representation and do not restore the deleted generator or generated
  dataclasses. In the existing authored definitions, make every
  always-serialized field required, leave only truly skipped fields such as
  `parent_span_id` and optional resource identifiers optional, and model
  `SpanStatus` as the three existing wire variants. Preserve native conversion,
  method signatures, field names, and runtime behavior.
- Focused proof: extend the existing Bifrost type-contract fixture (or the
  nearest public-type parity test) to pin the required/optional key sets and
  tagged status variants, then run `mise run py:typecheck` and the narrow
  Python unit test containing that assertion. The already-required Python
  action-timeline journey remains the runtime proof; no new harness or SDK
  conversion test is needed.

The replacement of generated Python dataclasses with authored TypedDicts is
not itself a finding: the new repository authority deliberately removed that
generator, nothing has shipped, and the native API already returns plain
dictionaries. Only the authored shape's mismatch with the canonical serialized
record needs correction.

## Verification limits

The implementation evidence records the strengthened Rust timeline test and
all three action-timeline journeys as green before the main merges. On the
current candidate, format, lint, typecheck, codegen, and docs checks are
recorded green, but the focused Rust timeline test and the Python and
TypeScript action-timeline journeys were not rerun after the merges. I did not
run Cargo or mise lanes in parallel with the other review agents. Source
inspection establishes FIND-2 closure and the current projections, but current
tip runtime execution remains for the orchestrator's verification decision.

## Overall result

**FAIL** — `FIND-TASK-001-2` is closed, the persistent query and Rust/TypeScript
projections remain correct, but `PD-R1-001` is a bounded public Python
data-contract regression introduced while adapting the task surface to the
post-merge authored-type system.
