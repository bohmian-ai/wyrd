# OTLP trace ingest CPU

Status: exploring

## Problem

OTLP protobuf trace ingestion repeatedly scans message framing, materializes an
owned request tree, clones resource/scope context per span, and builds standalone
Variant values before copying them into Arrow. The reported profile timings have
not been verified. Gate and Scribe also hash data at distinct identity boundaries;
those hashes cannot simply be substituted for one another.

## Desired outcome

Reduce CPU and temporary allocation for the existing trace ingest journey while
preserving accepted protobuf semantics, admission limits, canonical output,
tenant isolation, durable retry identity, and HTTP/gRPC behavior. Provide a
decision-complete proposal for all relevant costs, with separate measurement of
each change and the combined result.

## Constraints and non-goals

Design only. No production changes or load runs are authorized by this proposal.
No public API, storage schema, identity, or supported-input change is intended.

## Proposed implementation

### Scope and resulting journey

Optimize protobuf traces on both HTTP and gRPC. JSON traces and server-generated
traces use the same improved projection through their generated-message adapter.
Logs and metrics keep their present decoders; they receive only improvements in
shared helpers they already call. No new configuration, feature flag, transport,
crate, global cache, custom hasher, or storage representation is proposed.

An ordinary protobuf export follows this flow:

1. The existing transport authenticates and buffers one bounded request.
2. An allocation-free schema-aware preflight checks framing and limits, counts
   storage, and establishes whether direct borrowed decoding is safe.
3. The transport reserves capacity through the existing move-only
   `OtlpDecodeOwner`. gRPC retains its contiguous message as `Bytes` through
   `DecodeBuf::copy_to_bytes`; HTTP retains its existing body buffer.
4. One construction pass builds a compact request-local view of scalar fields,
   strings, bytes, attributes, events and links. Strings and byte payloads borrow
   the transport buffer. It does not construct an owned generated trace request.
5. Gate authorizes the table and resolves Card attribution under the existing
   rules. The table-owned projector consumes the borrowed view, validates rows,
   encodes attributes, and appends accepted rows to canonical Arrow columns.
6. Temporary views, resource/scope encodings, and the wire buffer are released
   before dispatch. Only owned canonical arrays, the outcome, attributed Cards,
   and the existing reservation cross into Scribe.
7. Gate derives its existing batch ID. Scribe stamps managed fields, partitions,
   derives its existing retry identities, admits, writes, and acknowledges at the
   same durability boundary as today.

The request's buffer lives in the adapter handler while its borrowed view is
passed to Gate, including across Gate's attribution await. There are no
self-referential structs, leaked lifetimes, unchecked UTF-8 conversions, or
borrowed buffers in a Scribe frame. The gRPC codec returns an owned prepared
payload; the handler creates the borrowed view after receiving it. JSON and
protobuf merge fallback retain their owned generated request through projection.

This changes representation and work, not scheduling or durable admission. It
adds no blocking IO, runtime, worker pool, or queue. Synchronous parsing remains
bounded by the existing request limits. This proposal does not claim to remove
all parsing CPU from Tokio request threads; c64 measurements must expose any
remaining scheduling cost.

### 1. Remove resource, scope, and row staging copies

Extend the existing trace column owner rather than adding a second projector.
Resource and scope envelopes are prepared once per corresponding group. Keep
those envelopes alive until the group's spans have been processed. Append their
borrowed strings and encoded Variant bytes to the destination columns for each
accepted span. This includes schema URLs, scope name/version, all resource
promotions, and nested entity references, not only the three commonly populated
resource strings.

Replace trace-side `Vec<String>`, `Vec<Vec<u8>>`, and per-row cloned
`EncodedVariant` staging where the destination can be built directly. Scalars
can retain simple numeric vectors when those already transfer into Arrow without
another copy. Existing canonical column order, nullability, nested list shapes,
and field/extension metadata remain authoritative.

Validation and fallible attribute encoding happen before scalar/string/context
columns commit a row. Event and link lists commit only with their parent span.
An invalid span leaves no child rows, list offsets, context entries, or accepted
row behind. Output-budget accounting still includes the repeated context bytes
for every accepted row: avoiding temporary clones does not make durable repeated
bytes disappear.

The existing generated-message projector remains an input adapter into the same
row validation and column writer. It is used by JSON, internal capture, and the
complex protobuf fallback. Generated and borrowed inputs must not maintain
separate promotion, correlation, validation, or error-selection implementations.
Use closed input views for their two representations; private type names and
iterator mechanics belong to implementation.

### 2. Build Variants by column and remove duplicate-key hash tables

Reuse the installed `parquet_variant_compute::VariantArrayBuilder`, which already
implements the nested builder interface used by `append_object` and
`append_any_value`. Extend the existing `wyrd_queue::VariantColumnBuilder` to own
this construction/validation path; do not add a competing signal-specific Variant
encoder. One multi-row builder per attribute column replaces standalone
`VariantBuilder` plus owned metadata/value vectors per attribute set. Resource
and scope attributes remain encoded once per group and appended repeatedly.

The installed array builder emits `BinaryView` children. Canonical Wyrd output
continues to use its existing `Binary` children and extension metadata. Add a
native `Binary` completion method to that existing fork builder: transfer its
metadata/value byte buffers into Binary arrays and construct checked i32 offset
buffers from its existing row boundaries. Wyrd applies its existing schema
wrapper. This avoids converting a complete BinaryView column into another copy
and does not change physical identity. The existing BinaryView completion path
remains intact for its other consumers. Overflow is fallible, not a truncating
cast; the existing request ceiling is not an excuse to omit that check.

Add the minimum transactional support in the pinned Arrow fork needed by this
existing builder: a checkpoint at a completed top-level value boundary, rollback
to that checkpoint, and read-only access to a just-completed value's metadata and
value slices. Preserve the Wyrd column builder's existing byte-copy append for
already-validated `EncodedVariant` values; repeated context is never decoded and
re-encoded just to append it. These operations remain within the existing builder;
they are not a general transaction framework. The Wyrd wrapper uses the existing no-copy
Variant validator on those slices. Variant error precedence, paths, numeric
rules, depth, and size limits are unchanged. Scribe still repeats its required
trust-boundary validation.

Before encoding a span's attributes, events, and links, checkpoint their Variant
columns. A failure restores all their row counts, bytes, offsets, validity, and
dictionary state, then returns the same row rejection. After all checks pass,
commit the other columns. Retained capacity after rollback remains charged;
there is no hidden memory released only in the accounting ledger.

Replace the per-object `HashSet<&str>` with reusable request-local index scratch:

1. Sort entry indices by key and original index.
2. Keep the greatest original index for each key: the last occurrence wins.
3. Visit the winning entries in descending original index, matching today's
   reverse traversal and preserving metadata dictionary assignment and bytes.

The winner set supplies distinct-key counts and final values to existing
promotion/correlation extractors, so they do not separately build a key set.
Nested objects use independent scratch frames, reused at the same depth after
completion. Scratch is bounded by admitted entry counts and the existing depth
limit. Worst-case work is O(n log n), including attacker-chosen keys; there is no
quadratic scan or unkeyed hash substitution. Preserve the distinct-key limit
versus entry-count limit distinction.

Make two narrowly related fixes in the existing fork metadata builder: look up a
borrowed `&str` before allocating a `String` for an already-known dictionary key,
and clear/reuse the dictionary table after serialization instead of discarding
its bucket allocation. Both preserve field IDs, insertion order, sorted flags,
and serialized bytes. Do not retain old dictionary entries between values.

Each value still has its own serialized dictionary and allocates owned names for
its distinct keys. Eliminating those names across rows would require a broader
dictionary/lifetime redesign or a shape cache. Neither is proposed: exact
encoding and bounded lifetime are more valuable than speculative cache savings.

### 3. Make common protobuf decoding two framing passes

Keep the current scanner as the shared framing owner. Refactor trace preflight
so a known message's fields are inspected together rather than once per desired
tag. Aggregate nested capacity facts once per subtree. Eliminate normal-case
`last_bytes_field`, selection, and nested recount passes.

Preflight records aggregate counts for the borrowed view's flat storage and a
single common-case eligibility result. A stack-local bitset per message tracks
occurrences of known singular fields. Direct borrowed decode is eligible only
when every known singular field appears at most once and each AnyValue has at
most one oneof occurrence. Repeated fields such as spans and attributes remain
ordinary input; duplicate attribute keys do not require decoder fallback.

On the first shape requiring merge/selection semantics, abandon the common-case
preflight and run the existing full preflight and generated decoder for the
whole request. Preserve that decoder's scalar last-wins, singular-message merge,
oneof run, repeated-member ordering, unknown-field, and group behavior. Do not
reject legal protobuf to simplify the fast path. This intentionally accepts a
small fallback penalty for unusual encodings rather than maintaining two merge
algorithms.

The common path performs one framing/capacity pass and one construction pass.
The construction pass uses preflight totals to allocate flat storage once per
element category. Group membership and nested values use ranges or indices into
those pools. Strings become checked `&str` once during construction; bytes remain
borrowed slices. It does not call `Scanned::of`, select AnyValue again, or count
repeated children to size per-message vectors. Projection subsequently visits
typed views, not protobuf framing. This is not a claim that every payload byte
is touched only twice: UTF-8 checking and encoding output still read payloads.

Flat borrowed views are needed because generated OTLP structs require owned
strings and bytes, while nested per-message vectors would need either recounts
or many growing allocations. They are an internal trace input representation
defined by the table owner and populated by the server decoder. They add no
wire contract, persisted type, public SDK export, or independent policy owner.
The server continues to own transport decoding; Vala continues to own projection.

### 4. Preserve both identity hashes and existing UTF-8 safety

No change to Gate's batch-ID digest or Scribe's per-partition retry digest is
proposed. They cover different values and boundaries. Passing the former as the
latter, combining independent column digests, hashing raw OTLP bytes, excluding
managed correlation, or changing the physical Variant representation would alter
identity semantics. A new digest protocol is outside this optimization.

The DEBUG-only per-column rehash remains diagnostic behavior. Baseline and
candidate performance runs use matching logging settings with that diagnostic
disabled; request DEBUG profiles cannot support production cost claims. This
proposal does not delete useful diagnostics solely to improve a debug capture.

There is no second Arrow UTF-8 validation to remove in the inspected string
builder path. Keep checked decoding. The borrowed representation stores valid
`&str`, so it requires no new unchecked conversion and introduces no repeated
range-to-string validation in the projector.

### Capacity and failure behavior

Continue enforcing both the wire ceiling and the existing expanded ceiling. On
the common path, preserve the legacy decoded-size eligibility calculation even
though generated backing is absent; this prevents an optimization from silently
accepting requests that the previous admission contract refused. Keep the
conservative projected-output bound, including resource/scope fan-out.

Reserve the actual peak capacities of the new representation before allocating:
flat view pools, duplicate-key scratch, Variant buffers and Arrow column
capacities, including offset buffers while existing boundaries are retained.
Reuse the existing owner and
scratch lease rather than admitting the request again. Extend that owner's
bounded resize/split operations only as needed; no alternate memory governor.
The reservation calculation uses checked arithmetic and actual allocation
capacity, not only current vector length. Transport buffer retention follows the
existing wire ownership and is counted once, not relabeled as generated memory.

Whole-request decode, capacity, or output-limit failure dispatches no batch.
Row-semantic failures preserve partial_success count, traversal ordinal, and
first rejection message. Resource/scope defects reject their descendants as
today. All-invalid and empty requests produce the same outcomes without a WAL
append. Gate must collect/resolve the same final record-level Card references
before row filtering, including references on rows that are later rejected.
Authentication, permission decisions, tenant lookup, audit staging, and registry
failure precedence remain at their existing boundaries.

Cancellation or any failure drops the wire buffer, view pools, scratch, columns,
and their reservation through ordinary ownership. No partial batch reaches
Scribe. If dispatch has already crossed into Scribe, its existing durable retry
and cancellation rules apply unchanged. No acknowledgement moves ahead of WAL
sync/fence; no publication or compaction work is added to the acknowledgement.

## Evidence and rationale

Inspected in this worktree; these are source facts, not measured savings:

- [Trace preflight and decoder](../../../../crates/wyrd/wyrd-server/src/otlp_decode.rs)
  contain repeated field scans, `Scanned::of`, AnyValue selection/recounts, and
  retained-string allocations. Logs and metrics import shared primitives here,
  so changes must preserve those callers.
- [Trace projection](../../../../crates/vala/vala-bifrost-redux/src/tables/traces/projection.rs)
  constructs resource/scope envelopes once but clones them per accepted span,
  stages owned strings and bytes, then assembles Arrow columns.
- [Signal projection](../../../../crates/vala/vala-bifrost-redux/src/tables/signal.rs)
  creates standalone Variant builders and per-object HashSets. Entity-reference
  column assembly also clones strings and key lists.
- [Variant owner](../../../../crates/shared/wyrd-queue/src/variant.rs)
  has the existing column builder and no-copy encoded-byte validator to extend.
- [Gate](../../../../crates/vala/vala-bifrost-redux/src/gate/mod.rs) derives its
  digest before stamping. [Scribe preprocessing](../../../../crates/vala/vala-bifrost-redux/src/scribe/preprocess.rs)
  hashes prepared partition slices after stamping. Its identity excludes only
  volatile receipt columns.
- The pinned Arrow fork is revision
  `394d4b0757488d70df9a78c0c271ab3c8fbc2928` in `Cargo.toml`. Its
  `parquet-variant-compute/src/variant_array_builder.rs` already supplies a
  multi-row nested builder. Its `WritableMetadataBuilder` allocates names before
  checking insertion and discards its dictionary table on finish. Its string
  builder finishes arrays without another UTF-8 validation.

The previous timing estimates and 638/685 sample counts remain unverified.
One percentage point is 0.1 ms only with a measured 10 ms CPU/request denominator
for that exact profile population. Inclusive parent/child frames overlap. Sample
shares describe current work, not removable time or additive predicted gains.

Authorities: [Bifrost identity/correlation, Variant, ingest resources and retry](../../../../architecture/bifrost-design.md),
[repository rules](../../../../AGENTS.md), [Rust shape](../../../../agent_docs/rust.md),
[test tiers](../../../../TESTING.md), and [benchmark rules](../../../../benchmarks/AGENTS.md).
The current [benchmark reader/workflow](../../../../benchmarks/README.md) governs
all measurements; no new recorder, runner, or record parser is needed.

## Implementation sequence and proof

These are cohesive delivery stages for later planning, not an approved task set.
Each implementation stage records its focused behavioral tests before production
changes, then compares its candidate with its immediate predecessor. The combined
candidate is also compared with the original baseline.

| Order | Change and owners | Completion evidence |
| --- | --- | --- |
| 0 | Reuse available baseline records; capture missing matched diagnostic baseline only through the capacity harness. | Valid c1/c64 reports, CPU/profile population, source/host/logging controls, and existing phase diagnostics identified. No guessed millisecond budget. |
| 1 | Trace column staging/context in `tables/traces/projection.rs`, shared signal helpers only where reused. | Exact batch equality and digest equality for multiple resource/scope groups, nested entities, mixed accepted/rejected spans, JSON/internal inputs; no per-span context clones in the measured path. |
| 2 | Fork builder Binary completion, checkpoint/rollback and metadata fixes; extend `wyrd-queue` Variant column owner; route signal attribute writers through it; remove duplicate-key HashSets in favor of shared scratch. | Per-cell byte equality, numeric/depth/size errors and paths unchanged, rollback after failure followed by success, exact canonical physical arrays and identities, bounded scratch. Logs/metrics/native users remain correct. |
| 3 | Trace preflight eligibility/counts and borrowed input views; HTTP/gRPC codec/handler integration and Gate input adaptation. | Two normal-path framing passes, no generated trace tree for eligible protobuf, fallback parity for merge cases, same limits/partial_success/attribution, wire owner released before Scribe. |
| 4 | Integrated regression and performance proof through existing journeys and capacity harness. | All applicable checks pass; CPU improvement is measured, c64 behavior and other affected write paths do not regress, durable readback and replay remain correct. |

The fork revision/lockfile update is one bounded dependency change with its own
tests, not hand edits under Cargo's cache. Follow the fork's contribution rules
and repository dependency workflow when implementing; publishing or pushing is
caller-owned. No new dependency or Cargo feature is required.

### Behavioral scenarios

New tests take their final names during implementation. The owners and critical
assertions are fixed here:

- **Projection unit tests:** use the existing maximal trace fixture plus multiple
  resources/scopes with different context, entity lists, null/empty fields, and
  rejected siblings. Compare every canonical array, nested list, ordering,
  extension, outcome and logical digest against the existing behavior. A failing
  late event/link Variant must not leave any parent or child column entry.
- **Variant owner/fork unit tests:** compare old standalone and new column
  encodings byte-for-byte for every AnyValue kind, duplicate and empty keys,
  nested repeated keys, null versus absent, finite/non-finite doubles, numeric
  boundaries, empty objects, size/depth boundaries, and field-ID width changes.
  Checkpoint, append several nested values, roll back, and append a valid row;
  the result must equal a clean builder. Repeated reset cycles retain no names
  or validity from prior rows. Exercise small and large buffers/offsets.
- **Decoder unit tests:** common-path output equals generated decode plus shared
  projection. Exercise field order, unknown fields/groups, truncation, malformed
  varints/lengths/wire types, UTF-8, request and nesting limits, and arithmetic
  overflow. Repeat each singular scalar/message and switch/repeat oneof members;
  all merge-required requests take fallback and preserve existing behavior.
  Include a late fallback trigger after a large valid prefix. Bench/test-only
  counters in the existing codec activity instrumentation establish path choice
  and absence of recount passes; do not add production per-field counters.
- **Transport/owner integration:** gRPC buffer ownership and HTTP body lifetime
  remain valid through attribution awaits and end before dispatch; malformed or
  expanded-limit input dispatches nothing. Cancellation, decode/projection error,
  all-invalid input and reservation refusal release the lease and scratch. Exact
  peak capacity includes buffer capacity and offset construction overlap.
- **Existing OTLP journey binary:** real HTTP protobuf, HTTP JSON and gRPC exports
  reach `WyrdTestServer` with repository Postgres, then canonical SQL readback.
  Extend the existing cross-transport and negative journeys for common/fallback
  equivalence, partial success, no orphan children, Card attribution including
  rejected rows, tenant denial, exact replay, and replay across definition and
  partition edits. Compare stored payloads and receipts, not only response codes.
  Native, Rust/Python/TypeScript SDK and MCP consumers retain their existing
  integrated Bifrost journey coverage; they gain no new APIs.

Use focused nextest commands for every test named during implementation. Existing
runners include `mise run test:bifrost:journey:otlp`,
`mise run test:bifrost:integration:redux`, and the affected owner unit suites
through `mise exec -- cargo nextest run --locked -p <owner> --lib` with focused
filters. Finish with `mise run verify:bifrost` for the integrated cross-owner
candidate. Run `codegen:check` as repository closeout requires; no generated
surface change is expected. A format/lint failure is fixed, not waived.

### Measurement and acceptance

Run on one quiet host, sequentially, with matching task flags, limits, logging,
fixtures, toolchain, shard count and benchmark plan. Keep production diagnostics
at matching non-DEBUG settings. Reuse existing phase telemetry and the capacity
readers before proposing any additional metric. Inspect existing records first.

Diagnostic comparison command for each stage:

```bash
mise run bench:l1:bifrost:writes -- --steps otlp-spans-1k-c1,otlp-spans-1k-c64,otlp-spans-c1,otlp-spans-c64
```

Use the same command and flags for both records; add `--allow-dirty` to both when
comparing dirty variants. For CPU attribution, run a separate matched diagnostic
pair with `--profile cpu`. Request heap profiling only when existing memory
readings cannot resolve the allocation question. Use the documented readers:

```bash
mise exec -- target/profiling/release/capacity report <record.json>
mise exec -- target/profiling/release/capacity compare <before.json> <after.json>
mise exec -- target/profiling/release/capacity profile profiles/<step>/cpu-server-<pid>.json.gz
```

Resolve the profile path from the capture produced by the run, as documented in
`benchmarks/README.md`. No hand-parsed snapshots or custom profile readers.

Judge CPU per row and rows/wire MiB per CPU-second first. Read acknowledged
throughput, server p50/p95/p99, errors/refusals, resident/governed memory, backlog,
and reconciliation alongside them. A profile's reduced share is explanatory;
whole-server CPU includes the background work caused by that window and remains
the practical cost check.

For a stage to land as a performance optimization, its intended CPU or allocation
improvement must be observed with valid evidence; a CPU improvement claim requires
non-overlapping repetition spreads in the capacity comparison. A difference
inside spread is no measured change. Do not add estimated savings from stages.
An outside-spread adverse change to c64 CPU/throughput or peak memory, or a
consistent adverse shift in server latency buckets, requires investigation and
resolution before accepting the stage. Do not silently accept a new CPU/memory
tradeoff. If a more complex stage has no demonstrated benefit, omit it and
record that result; do not keep machinery to satisfy an optimization checklist.

The final clean candidate runs the full `mise run bench:l1:bifrost` suite and
`mise run bench:l2:bifrost` with the normal compatible calibration, compared with
matched clean baseline records. These establish the integrated result beyond
selected steps and catch shared Variant regressions in logs/native writes and
downstream reads. Narrowed, dirty or profiled results remain diagnostic, not
qualification. Report each run in the six-section format required by
`benchmarks/AGENTS.md`, pasting report/compare output without invented numbers.

No percentage or millisecond saving is promised by this proposal.

## Decisions and alternatives

- Cover every confirmed removable scan/copy/allocation pattern together, but land
  and measure in stages. Do not attempt an unsafe digest substitution for #5.
- Use borrowed payloads and bounded flat views instead of generated owned traces
  for common protobuf. Retain whole-request fallback to the existing decoder;
  implementing another merge algorithm has higher maintenance and correctness
  cost without evidence that unusual exporters justify it.
- Extend the existing projection and Variant owners. The current generated
  structs cannot borrow payloads, and the current standalone Variant builder
  cannot commit multiple rows or roll back a failed parent span; those concrete
  limitations justify the new views and small fork builder changes.
- Preserve exact serialized Variant bytes and canonical physical arrays. Cross-row
  dictionaries, BinaryView canonical storage, dictionary Arrow columns and raw-wire
  batch IDs are excluded because they change identity or storage behavior.
- Preserve required byte validation and checked text. No custom hasher, linear
  quadratic duplicate scan, global context intern pool, or unbounded shape cache.
- Add native Binary completion to the existing multi-row builder rather than
  introduce a second Wyrd serializer or copy complete columns during a cast. The
  fork already needs bounded rollback for row rejection; this additional method
  reuses its buffers and boundaries without adding another state owner.

These are recommendations awaiting maintainer agreement, not previously
approved implementation requirements. Symbol/file splits, iterator mechanics,
builder checkpoint representation and focused test names are implementation-owned
provided the behavior, ownership, bounds, and evidence above remain intact.

## Standards conflicts

None identified. The proposal preserves the table-owned projection, server-owned
transport, root-backed memory, exact Variant contract and durable identities.
An implementation discovery that requires changing accepted input, identity,
errors, resource guarantees, ownership tier or durable behavior returns to design
and specification; it is not a private performance choice.

## Open questions

No material technical decision is intentionally left to the implementer. Actual
CPU/allocation benefit and whether every proposed stage earns its complexity are
empirical acceptance questions, not assumed facts. Fork API implementation is
subject to its own tests and review, not an assertion that those APIs exist now.

## Handoff

Proposal complete for maintainer discussion; status remains exploring until the
maintainer agrees. No specification, task packet, production edit, benchmark,
commit or push was performed. After agreement, finalize these notes; specification
and planning remain separate requested steps under the repository workflow.
