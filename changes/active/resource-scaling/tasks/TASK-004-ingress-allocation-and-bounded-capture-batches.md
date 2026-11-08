---
id: TASK-004
kind: implementation
status: proposed
spec: SPEC-resource-scaling
spec_revision: 4
requirements: [REQ-003, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008, AC-002, AC-003, AC-007, AC-008, AC-009, AC-010]
depends_on: [TASK-003]
---

# Reduce ingress allocation and build bounded capture batches

## Outcome and Value

Reduce the short-lived payload buffers still allocated after the native transport
and sampler work. A captured 64 KiB request plus 64 KiB response still allocates
about 2.6 MiB per reconciled call in the supplied profile. The remaining large
copies occur in Gateway preparation, capture batch construction and learned
ingress shredding, rather than a repeated final-Parquet reconstruction cycle.

For example, the outbox currently concatenates capture payloads into a large
batch, then copies its rows into bounded delivery frames. Construct the bounded
groups first, so each dense batch can become a frame without that second copy.
Keep parallel per-capture encoding and its one concatenation per bounded group.

This is the follow-on requested on 2026-11-08. TASK-003 already contains the
implemented transport/sampler change and its evidence; preserve that identity.
The maintainer agreed to the ordered scope below. It extends the same approved
allocation outcomes without adding a public or persisted contract.

## What Changes and What Must Stay True

Implement the entire sequence, in order:

1. Remove temporary JSON text used only to calculate token bounds; reduce native
   serialization growth and validation-only copies of request strings.
2. Supply appropriate byte capacity to existing ingress residual builders.
3. Reuse eligible untouched residual buffers, accounting their complete backing
   allocations. A residual is the Variant content not represented by typed leaves.
4. Construct bounded capture groups before dense projection and frame splitting.
5. Apply safe capacity hints to the existing bounded provider-response collector.
6. Measure the integrated result with paired profiles, then full unprofiled L1.

Preserve accepted/rejected inputs and stable errors; model substitution; unknown
and flattened fields; selected capture content, redaction and binary references;
native response bytes; translation, streaming, retries, fallback and cancellation.
Keep the original ingress JSON value. Do not replace it with a typed-only request
or a second parser. Validation must remain tied to the existing request contracts,
not an independently maintained list of required fields.

Capacity is not free memory. Account input/output overlap and full retained Arrow
buffer capacity, including shared parents, under existing global/tenant/table
owners. Do not remove final trimming just to lower cumulative allocation, reserve
the full payload independently at every nested node, or retain a large parent
while charging only its small slice. Admitted rows must remain processable; no
new oversize escape, reduced payload limit or lost capture is allowed.

Buffer sharing preserves logical values and existing physical validity semantics.
It need not reproduce the previous encoder's byte order: a source object's value
offsets may differ from the rebuilt object's offsets. Retries of the same claim
or frozen outbox slice must still produce identical bytes/identities. Preserve
the metadata dictionary each borrowed value references. An incompatible selected
field still needs its own residual slot; absence of typed output alone does not
make a complete object eligible for passthrough.

Keep canonical WAL, ACK-before-handoff, staged recovery, publication fencing,
tenant attribution and all learned reads. Every new hot-file claim still takes
the existing fresh seeded/stratified sample. No layout thresholds, sampling
observations, compaction policy or final compatible-layout passthrough change.

Outbox writers, lanes, linger, delivery limits, queue capacities and admission/loss
policy stay fixed. Preprojection grouping implements the existing bounded-frame
policy earlier; it is not batching retuning under INV-004. Preserve its budget,
oversized-single-row behavior, row order, detail-before-summary, owner attribution,
ACK handling and deterministic local/peer retry behavior. Newly created slices
need not reproduce another software release's frame IDs. One frozen slice must
reconstruct the same boundaries and IDs on every attempt, regardless of route.

Explicitly defer Variant header reservation/backfilling, moving capture encoding
onto the outbox lane, typed-only ingress, Hyper connection tuning, WAL IPC/zstd
changes, merge/interleave tuning, translated-request double preparation, pools,
caches, allocator changes and new dependencies/features. Preserve one compatible
Arrow/Iceberg stack; compatible immutable fork repins are part of this task.

## Approach

1. Establish the immutable immediate baseline and confirm the inspected call paths.
2. Remove Gateway sizing/validation materialization and reduce native serialization
   allocation through existing owners, including its final boxing/transport step.
3. Improve residual capacity, then add ownership-aware passthrough in the existing
   Arrow compute owner; integrate compatible canonical fork pins.
4. Project captures in bounded groups through the existing outbox/capture owners,
   retaining parallel capture encoding and one dense concatenation per group.
5. Add bounded response-read capacity hints without changing transport semantics.
6. Run focused and owner verification, paired allocation profiles and final L1.

## Ordered Implementation Scenarios

### Scenario 1 — Token bounds do not require a serialized request buffer

**Behavior.** For a valid JSON request, input-token bounds equal its compact JSON
UTF-8 byte length, including escaping, Unicode, nested values and numeric forms.
Output maxima, absent maxima and unrepresentable quantities retain their existing
outcomes. A large string is counted without allocating its serialized text.

**RED.** Extend the [sizing proof](#gateway-sizing-and-validation) with exact
reference-length cases and a focused allocation observation for a large payload.
The old implementation fails the materialization assertion, not value parity.

**GREEN.** Count through existing serialization semantics without retaining the
output; preserve checked quantities and the current error/absence behavior.

**REFACTOR.** Keep counting at the existing usage-bound owner; avoid a new JSON
parser or tokenization policy. Rerun sizing cases after simplification.

### Scenario 2 — Native serialization avoids unnecessary growth copies

**Behavior.** Native requests serialize once with lower allocation across buffer
growth and final ownership conversion. Model replacement, unknown fields,
escaping, credentials and exact transport content remain correct; retries share
the completed body. Capacity estimates remain hints when output size differs.

**RED.** Extend the [native proof](#native-serialization) with a large-body
allocation observation and model/extension/escape cases. Assert equivalent final
bytes and reduced temporary allocation; the current serializer fails the latter.

**GREEN.** Improve capacity/ownership through the existing native serialization
workflow, including final boxing. Do not introduce a second serialization merely
to size the first. Rerun scenario 1 and transport ownership/bound regressions.

**REFACTOR.** Reuse the same native encoding path across its consumers. Private
capacity estimation and buffer types remain implementation choices.

### Scenario 3 — Typed validation does not copy payload strings to discard them

**Behavior.** Request-contract validation preserves its accepted shapes, unknown
fields and missing/mistyped-field errors before authorization/dispatch. It keeps
the original JSON for forwarding/capture without allocating another body-sized
owned representation just to discard the validation result. All typed-body
consumers retain their contract, including non-chat JSON operations.

**RED.** Add the planned [validation proof](#gateway-sizing-and-validation): valid,
missing/mistyped, unknown/flattened and nested request cases compared with current
contract decoding, plus an allocation observation for a long message. Old owned
decoding fails the payload-copy assertion while establishing reference outcomes.

**GREEN.** Extend the existing contract/validation owner to avoid transient payload
ownership without parallel handwritten validators or parser replacement. Rerun
scenarios 1–2 and affected request-contract tests.

**REFACTOR.** Remove duplicate conversion while keeping validation authoritative
in the shared contract. No public SDK/schema shape changes belong here.

### Scenario 4 — Residual construction uses byte-aware capacity

**Behavior.** Learned ingress shredding reduces growth allocation for large
residuals with unchanged values/layouts. Typed-heavy, residual-heavy, nested and
list cases do not retain multiplied whole-input reservations. Existing trimming
and byte accounting remain effective.

**RED.** Add the planned [residual proof](#residual-encoding): compare decoded
values and physical validity with the current implementation, and observe growth
allocation/retained capacity on large residual and mostly-typed cases. The current
zero-byte-hint path fails the allocation assertion.

**GREEN.** Use existing byte-capacity support with estimates appropriate to each
residual output. Preserve bounds and finalization. Rerun earlier scenarios.

**REFACTOR.** Keep estimation in the existing compute owner. Do not add a separate
encoding stack or change object/list header encoding.

### Scenario 5 — Untouched residuals share storage without hidden retention

**Behavior.** Eligible untouched values reuse existing bytes while partially
transformed values retain correct typed/residual decomposition. Exercise mixed
rows, selected fields with incompatible types, SQL null, missing and explicit
null, empty objects, primitives, lists and nested partial objects. Preserve each
row's metadata references and deterministic retries. Shared small values are
charged for any large backing buffers they retain.

**RED.** Extend the [residual proof](#residual-encoding) after scenario 4: assert
buffer sharing for eligible rows, logical/physical validity parity for all cases,
and correct retained-capacity accounting. The copying implementation fails sharing.
Do not require byte equality with the old rebuilding encoder.

**GREEN.** Extend the existing Arrow compute/view owners to mix shared eligible
values and rebuilt residuals. Preserve memory accounting across Scribe ownership
transitions and use existing rebuilding where sharing cannot meet those bounds.

**REFACTOR.** Reuse buffer-identity accounting and existing view builders; keep one
metadata/validity interpretation. Rerun scenarios 1–4 and learned-layout proofs.

### Scenario 6 — Captures become bounded batches before delivery copies

**Behavior.** Large captures are grouped before dense construction. Each ordinary
group produces a bounded independently owned delivery batch with one payload
concatenation and no subsequent split-copy. Sizing includes scalar, metadata and
attribution columns. Skewed payloads, metadata-only/null payloads, attempts and
mixed owners preserve order, attribution and exact admitted content. A single
oversized row retains the existing whole-row submission/refusal behavior.

**RED.** Extend the [frame proof](#bounded-capture-batches) with multiple large
captures and skewed sizes. Observe construction/copy allocation as well as frame
size, exact readback and retry/local-peer parity. The existing dense-then-split
path fails the additional-copy assertion. Preserve existing negative assertions.

**GREEN.** Extend existing projection/grouping owners to construct under-budget
groups before dense batching, using complete size accounting. Keep deterministic
boundaries per frozen slice, owner positions, detail-before-summary and delivery
limits. Keep current safe splitting for unrelated already-projected batch callers;
do not replace independent copies with undercharged parent slices.

**REFACTOR.** Remove only capture-path work made redundant by earlier grouping.
Do not move capture encoding onto the lane, change linger or introduce another
outbox lifecycle owner. Rerun scenarios 1–5 and retry/cancellation regressions.

### Scenario 7 — Response capacity hints preserve bounded transport behavior

**Behavior.** Known response lengths reduce collection growth; unknown/chunked
lengths still work. Huge advertised lengths never cause unbounded reservation.
Actual bytes still enforce the current limit, and truncation, invalid UTF-8,
upstream status and cancellation retain existing behavior.

**RED.** Extend the [response proof](#bounded-response-collection) with known,
unknown, oversized and truncated lengths, and a focused capacity/allocation
observation. The current empty collector fails only the capacity improvement.

**GREEN.** Use a safely converted and bounded hint in the current response reader;
retain chunk-by-chunk actual-byte enforcement and errors. Rerun scenarios 1–6.

**REFACTOR.** Keep this local to bounded collection. Do not replace server ingress
chunk collection or attribute Hyper connection-buffer savings to this change.

## Acceptance Criteria

| Approved obligations | Required observation and proof |
|---|---|
| REQ-010; AC-008; INV-008 | Scenarios 1–3 and 7 remove observed temporary allocation while preserving exact sizing, contract validation, provider body semantics and response bounds. Existing selected-capture/redaction and transport tests stay green. |
| REQ-008/009/011; INV-003/005/007; AC-007/009 | Scenarios 4–5 preserve logical values, physical validity, fresh learning and compatible-layout reuse; full shared-buffer capacity and conversion overlap remain charged. The learned-layout journey and redux owner lane remain green. |
| REQ-003/010/011; INV-004/006/008; AC-002/008/009 | Scenario 6 proves bounded independently owned batches without the extra split-copy; retry IDs/content, local/peer routing, attribution, oversized-row handling and cancellation counts remain exact. No batching/concurrency policy is retuned. |
| REQ-012; AC-003/010; INV-008 | Paired profiles demonstrate attributable allocation reduction, separate from retained memory. Final L1 records every engine/step/repetition and zero errors/lost/shed/undrained work. Immediate-baseline regressions are investigated; original whole-change baseline/main/read obligations are not reset or waived. |

## Implementation Reference

Research inspected Wyrd `a67efcd8dee0df55886d4b3deaf7d8e8a1070c93`; the supplied
TASK-003 performance candidate is `aa2a6a0d8`. Revalidate changed owners before
editing. Guidance below is not a file/type allowlist.

- `wyrd-server/src/components/gateway/routes.rs::token_bound` currently calls
  `body.to_string().len()`; `usage_bound` consumes that length. `typed_body`
  deserializes an owned shared request from `&Value`, discards it and returns the
  original value. Inspect every typed-body caller and the existing request types
  before choosing an ownership-preserving validation extension.
- `wyrd-gateway/src/adapter/http.rs::NativeBody` and `send` own one native
  `to_raw_value` conversion. `skald-providers/src/raw.rs` and
  `clients/mod.rs::send_bytes_with_retry` already transfer/share the encoded body.
  Reuse that workflow; include boxing/shrink costs in evidence. Original wire
  length is not currently carried through the JSON extractor and is not an exact
  output size after model rewriting; do not add a public length contract.
- `wyrd-server/src/components/gateway/capture.rs::redacted_payload` already uses
  `EncodedVariant::from_serialize`; queue `variant/serialize.rs::Encoder` writes
  direct Variant. Object/list finish inserts headers into a shared buffer, not
  separately owned child payloads. No intermediate-JSON removal is needed here.
- Pinned Arrow `ca5feff3b1c523640a03acaa437096b8934c3749`, package
  `bohmian-parquet-variant-compute`, owns `shred_variant.rs` and
  `variant_array_builder.rs::VariantValueArrayBuilder::with_capacity`. Shredding
  currently initializes residual builders with zero byte hints. Object residual
  `insert_bytes` reaches `ValueBuilder::append_variant_bytes`. Extend these owners.
  Existing Arrow byte-view builders support shared blocks; preserve row metadata
  and validity rather than treating all no-typed-output rows as interchangeable.
- `scribe/shards.rs::bind_slice_layout` and `scribe/layout_cache.rs` apply cached
  layouts before retaining memory. `scribe/memory.rs::retained_arrow_bytes` already
  identifies backing allocations/capacity; reuse its accounting. Final claim
  sampling/merge/writing and their reservations remain unchanged except necessary
  truthful shared-storage accounting.
- `capture.rs::CallCapture::project/calls_batch` concatenates one-row Variant
  columns. `scribe_outbox.rs::ScribeSink::frames/chunks`, `Part::dense`,
  `Chunk::encode/split/identify` currently project first, split later and use local
  `take_record_batch` to give split frames independent backing. Extend those
  existing owners to group before projection; payload byte length alone is not a
  complete frame bound. Unchanged scalar-row and other projected-batch consumers
  must keep safe behavior. Retry identity includes frame/chunk/start positions.
- `skald-providers/src/clients/mod.rs::read_bounded_bytes` starts at `Vec::new`.
  Server `http/middleware/body_limit.rs` already collects immutable chunks; no
  replacement is needed. Profiled Hyper `reserve_inner` is connection I/O storage,
  not this response vector.
- Candidate profile `20261107T211606Z`, payload c64 minus preceding c1: token-bound
  text ~19.7 GiB; native serialization ~26.3 GiB; capture concat ~13.1 GiB;
  outbox take ~13.2 GiB. Ingress shred includes its builder descendants. These are
  attribution from one interval, not additive saving promises or new medians.

## Verification and Evidence

Keep proof small: extend existing cases where possible. Allocation observations
must distinguish the old/new behavior with deterministic, isolated accounting or
an existing focused profiling facility, not brittle absolute allocator totals.
Avoid a new benchmark/test framework. Confirm all new selectors after adding
tests, before RED; names marked planned below are not existing tests.

All Wyrd commands use default features unless the existing owner requires more.
Run Cargo commands sequentially. No new external surface is added; reuse real
journeys rather than duplicating language suites. Broad change-review gates remain.

### Gateway sizing and validation

Existing sizing proof, extended for scenario 1; planned validation proof for
scenario 3. Both are credential-free server library tests:

```sh
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::routes::usage_bound_tests::usage_bound_requires_an_output_maximum_where_output_exists)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::routes::usage_bound_tests::request_validation_preserves_contract_without_payload_copies)'
```

### Native serialization

Extend this existing native-body regression for scenario 2:

```sh
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=adapter::tests::compatible_passthrough_rewrites_model_and_uses_provider_credential)'
```

### Residual encoding

Use an isolated writable Arrow checkout based on the pinned source, never edit
Cargo's cache or overwrite an unrelated local checkout. Read its applicable
AGENTS/CONTRIBUTING first. Set `WYRD_ARROW_WORKTREE` to its recorded absolute root.
The inspected package/library has these credential-free tests. The first selector
is **planned**, extended in sequence for scenarios 4 and 5:

```sh
mise exec -- cargo nextest run --locked --manifest-path "$WYRD_ARROW_WORKTREE/Cargo.toml" -p bohmian-parquet-variant-compute --lib -E 'test(=shred_variant::tests::residual_encoding_reuses_storage_with_bounded_capacity)'
mise exec -- cargo nextest run --locked --manifest-path "$WYRD_ARROW_WORKTREE/Cargo.toml" -p bohmian-parquet-variant-compute --lib -E 'test(=shred_variant::tests::test_object_shredding_comprehensive)'
mise exec -- cargo nextest run --locked --manifest-path "$WYRD_ARROW_WORKTREE/Cargo.toml" -p bohmian-parquet-variant-compute --lib -E 'test(=shred_variant::tests::test_object_shredding_with_array_field)'
mise exec -- cargo nextest run --locked --manifest-path "$WYRD_ARROW_WORKTREE/Cargo.toml" -p bohmian-parquet-variant-compute --lib -E 'test(=shred_variant::tests::test_object_different_schemas)'
```

Run the changed fork package's full library tests and documented format/lint
checks; record actual commands/toolchain. Any necessary accounting regression
belongs at the existing Wyrd memory owner; record its exact selector before RED.
Integrate under canonical immutable fork SHAs in Cargo.toml/Cargo.lock, coordinating
existing Arrow/Iceberg pins as needed. No new dependency family, major version or
feature is authorized. Git commit/push scheduling remains caller-owned; temporary
path overrides are not a completed integration.

### Bounded capture batches

Extend the existing capture/bounded-frame cases for scenario 6. The remaining
selectors are preservation proof, not manufactured RED scenarios:

```sh
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::capture::tests::capture_columns_avoid_intermediate_payload_copies)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=scribe_outbox::tests::an_oversized_detail_reaches_scribe_in_bounded_frames_that_replay_identically)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=scribe_outbox::tests::local_canonical_and_peer_ipc_preserve_frames_and_replay)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=scribe_outbox::tests::a_retry_resubmits_identical_frames)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=scribe_outbox::tests::an_unsplittable_row_is_submitted_whole_and_terminal_refusal_consumes_it)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=scribe_outbox::tests::cancelled_encoding_retains_delivery_capacity)'
```

### Bounded response collection

Extend the existing bound/error case for scenario 7; preserve transport ownership:

```sh
mise exec -- cargo nextest run --locked -p wyrd-skald-providers --lib -E 'test(=clients::bounded_answers::oversized_answers_fail_and_refusals_keep_status_and_body)'
mise exec -- cargo nextest run --locked -p wyrd-skald-providers --lib -E 'test(=clients::bounded_answers::native_transport_reuses_owned_buffers)'
```

### Integration and owner checks

Reuse the real SDK/server learned-layout journey to prove layout evolution and
published values under the new fork. Existing Gateway journeys below prove
streaming/capture isolation, cancellation settlement and attributed split-role
submission after drain. Extend the peer case with a bounded large-capture group
if needed to expose the changed batching seam through real readback. Do not
substitute a mock-only frame test for that real lifecycle proof.

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(=write_read::learned_layout_evolution_preserves_published_evidence)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=peer::oracle_only_gateway_captures_through_the_peer_scribe)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=compatible::openai_compatible_streams_terminate_and_survive_a_capture_outage)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=resilience::a_cancelled_call_is_still_settled_and_the_server_keeps_serving)"'
mise run test:bifrost:integration:redux
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib
mise exec -- cargo nextest run --locked -p wyrd-skald-providers --lib
```

Run affected server library modules (routes usage bounds, capture and outbox):
`mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^components::gateway::routes::usage_bound_tests::/) or test(/^components::gateway::capture::tests::/) or test(/^scribe_outbox::tests::/)'`.
Keep any separately discovered database-backed tests in their repository-managed
setup. Run `mise run fmt:check`, `mise run lints`,
`mise run check:deps`, `mise run check:tenant-isolation` and
`mise exec -- git diff --check`. No codegen changes are expected; a discovered
generated/public-contract change is a scope conflict, not incidental plumbing.

### Paired profiles, then final full L1

Use immutable `aa2a6a0d8484becc9d8cc1bf4d90e6b763aef828` as the immediate
performance baseline. Inspected HEAD differs only by TASK-003 evidence text;
production sources are identical. Document any subsequent baseline correction.
Keep original pre-TASK-002 and main comparisons separate; this task does not
redefine spec acceptance.

1. After checks pass, run baseline/candidate
   `mise run bench:l1:gateway -- --allow-dirty --profile` on the same quiet host,
   identical settings/builds. Use at least two matched intervals per side. Match
   cumulative allocation deltas with succeeded-call deltas covering the same
   warmup/measured/drain population. Report payload c1/c64 and matched metadata.
   Confirm sizing materialization and capture split-copy disappear, native/residual
   growth declines, and safe shared residuals avoid copies. Keep inclusive parent
   stacks separate from leaf totals; report any remaining concat explicitly.
2. Finish with baseline/candidate full unprofiled
   `mise run bench:l1 -- --allow-dirty`, all five engine experiments and all three
   repetitions. No overlapping builds, tests or other load. Retain raw records,
   timings and profiles outside disposable worktrees. The old baseline's missing
   Bifrost/Verification records cannot substitute for these new paired records.
3. Preserve AC-003's main comparison and one fresh back-to-back rerun rule when
   triggered. Report existing engine verdicts and targets exactly. Verification
   driver saturation remains INVALID until resolved through its owning behavior;
   do not relabel it VALID or widen thresholds within this task.

Record full source/fork SHAs, dirty diff hashes, resources/outbox geometry,
artifact locations, elapsed/drain work, zero errors/lost/shed/undrained, population,
allocation, CPU/rate/p99, peak RSS/time series, live heap, retained capacity and
post-drain ownership across repeated equivalent cycles. Seek a repeatable further
allocation reduction beyond variation without RSS/performance regression relative
to the immediate baseline. The approved whole-change AC-010 allocation/RSS target
still uses its original baseline and requires closure there; no new absolute floor
or guaranteed additional RSS reduction is invented here. Missing/invalid proof
is reported as such, never waived by an attractive allocation total.

## Material Stop Conditions

- Validation cannot avoid payload copies without changing accepted/rejected
  requests, error semantics, public contract types or duplicating the parser:
  finish unaffected work and return the exact contract/authority gap.
- Residual reuse changes values, metadata interpretation, physical validity,
  sampling, persisted representation or deterministic retries; or requires
  undercounted retention, raised budgets or removal of trimming to show savings.
- Earlier grouping requires batching/concurrency-policy retuning, altered
  admission/loss/oversize behavior, nondeterministic route-dependent frames or
  weakened owner attribution. INV-004's saturation evidence remains mandatory for
  an actual policy change; none is authorized by this construction optimization.
- A new dependency/feature, major version, ownership tier, public surface or
  durable format is necessary, or compatible immutable fork integration lacks
  caller-owned Git authority. Do not ship Cargo cache edits/path patches as done.
- Acknowledged work cannot make progress with truthful full-capacity accounting,
  a required gate remains red, or measurement populations/artifacts cannot be
  reconciled. Diagnose through normal execution recovery; no performance waiver.

Private helpers, capacity heuristics, equivalent focused selectors, fixture
structure and reversible local organization remain implementation-owned.

## Authority Links

- [Approved resource-scaling spec, revision 4](../spec.md)
- [Implemented transport/sampler task and evidence](TASK-003-native-transport-and-sampler-allocation.md)
- [Repository rules](../../../../AGENTS.md)
- [Spec-driven development](../../../spec-driven-development.md)
- [Implementation execution](../../../implementation-execution.md)
- [Testing](../../../../TESTING.md)
- [Bifrost sampling, memory and durability](../../../../architecture/bifrost-design.md)
- [Security posture](../../../../architecture/wyrd-security-posture.md)
- [Benchmark method](../../../../benchmarks/README.md)
