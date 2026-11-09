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

## Scope Amendment (review decision)

Recorded from the TASK-004 review. It amends this task only; the spec's
original AC-003 and AC-010 comparisons are unchanged.

- **Scenario 3 is removed.** Public request types and typed validation stay as
  they are. The helper validates six operations through owning types, and
  flattened provider settings can carry large values. The measured cost, about
  2.1 MiB allocated for a 2 MiB body, remains a known cost. A future design
  should give validation one authoritative path that can borrow. No second
  handwritten validator and no public ownership change.
- **The immediate-baseline memory comparison runs at matched load.** Use the
  optional `--pace` diagnostic: same L1 payload, tenant, upstream and capture
  policy; fresh server processes; the same offered rate on both sides (90% of
  the lowest valid baseline sustained rate, at most 64 calls in flight);
  identical warmup and workload; three load/drain cycles. It must record zero
  errors, missed or capped arrivals, lost or shed captures and undrained work,
  plus opening, peak and post-drain RSS and governed memory by owner.
  Allocation per call and live heap come from separate profiled runs. Default
  L1 is unchanged.
- **Memory acceptance.** Higher RSS at the candidate's higher closed-loop
  throughput is accepted only if all of these hold:
  - matched-load peak RSS is within baseline variation or better;
  - governed ownership stays within existing budgets;
  - post-drain live ownership settles across cycles;
  - the original pre-TASK-002 allocation/RSS requirement passes;
  - Gateway's comparison against main passes.

  Otherwise the retaining owner is found and fixed. No raised budgets, and
  memory per captured GiB is not a substitute.
- **Bifrost logs.** AC-003's one-rerun rule covers Gateway against main only.
  For `otlp-logs-c64`, run two predetermined pairs, alternating the order of
  baseline and candidate, three repetitions each, and report every result. A
  consistent slowdown is profiled and fixed. Inconsistent results remain an
  explicit unsettled finding.
- **Verification.** Records that are INVALID for driver saturation stay
  INVALID. Valid evidence needs lower driver overhead at its owner, with the
  same workload and saturation threshold.

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

## Implementation Evidence

Candidate `bea7f8b4c` (Wyrd `78aad72c9`, `fcfe12740`, `6a134deb8`, `b3a6363a4`,
`bea7f8b4c`) on immediate baseline `aa2a6a0d8`. Arrow fork on `wyrd-production`:
`394d4b0757488d70df9a78c0c271ab3c8fbc2928` (`a11acb8` scenario 4, `394d4b0`
scenario 5, on `ca5feff`), pinned in `Cargo.toml`, `Cargo.lock` and the
workspace-hack. Every Wyrd benchmark record is `dirty: false`; the only
untracked path is an unrelated change packet. Host: Ryzen 9 9950X, 32 CPU /
91 GiB, Linux x86_64, rustc 1.99.0, release with frame pointers.

### Reuse map

- **Token bound:** `routes.rs::token_bound` still measures the serialized body
  length that `usage_bound` consumes. It now counts it through
  `serde_json::to_writer` into a byte counter (`SerializedLength`), so the
  serialization semantics are unchanged and no text is built.
- **Native body:** `adapter/http.rs::NativeBody` keeps its single serializer.
  `to_json` estimates the exact length from the `Value` tree and serializes once
  into a `Vec` of that capacity. The providers' existing `send_raw`/`stream_raw`
  take ownership of that `Vec` (no `RawValue` box, no shrink). No length
  contract was added.
- **Residuals:** the fork's `VariantValueArrayBuilder` reserves the top-level
  residual from the input's row lengths (`reserve_for_row`). A top-level row
  whose stored value bytes need no rewrite (no shredded field present, or no
  typed output) is referenced in the input buffer (`append_shared`). `build`
  keeps a shared buffer only when the rows reference at least half of it, and
  otherwise copies those rows. Wyrd's `retained_arrow_bytes` already charges full
  buffer capacity, so a kept buffer is charged in full. Nested residuals,
  sampling, trimming and reservations are unchanged.
- **Capture batches:** `ScribeSink::frames` groups captures greedily by
  `CallCapture::row_bytes` plus the attributed columns before projecting. Each
  group is projected once by the existing `CallCapture::project`, which falls
  back to per-capture projection. The `chunks` split then cuts already bounded
  batches and no longer needs `take_record_batch`. Batch sizes, concurrency,
  admission, loss and the oversize policy are unchanged.
- **Response collection:** `read_bounded_bytes` sizes its `Vec` from
  `Content-Length` capped at the bound. Actual-byte enforcement per chunk is
  unchanged. Server ingress body collection was not touched.
- **Validation (scenario 3): not changed.** `typed_body` deserializes
  `GatewayChatCompletionsRequest` (skald-spec) from `&Value`. That type owns
  `String`/`Value` fields and flattens provider settings, so any typed check
  materializes the payload. Avoiding the copy would mean either borrowing public
  contract types or a second validator that duplicates the parser. Both are
  stop conditions.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-010; AC-008 (scenario 1) | `routes.rs::token_bound`, `SerializedLength` | `usage_bound_tests::usage_bound_requires_an_output_maximum_where_output_exists` extended: exact length parity on escapes, unicode, numbers and a 2 MiB body; RED 3,146,387 B allocated, GREEN < 4096 B | PASS |
| REQ-010; AC-008 (scenario 2) | `adapter/http.rs::NativeBody::to_json`, providers `send_raw`/`stream_raw` take `Vec<u8>` | `adapter::tests::compatible_passthrough_rewrites_model_and_uses_provider_credential` extended: byte parity with `serde_json::to_vec` on a 1 MiB escaped body with extensions, with and without a model; capacity equals the up-front estimate and ≤ len + len/16 (RED: buffer grew) | PASS |
| REQ-010; AC-008 (scenario 3) | removed by the scope amendment | `typed_body::<GatewayChatCompletionsRequest>` copies 2,098,793 B for a 2 MiB body (focused measurement), recorded as a known cost | OUT OF SCOPE |
| REQ-008/009/011; INV-003/005/007; AC-007/009 (scenarios 4–5) | fork `shred_variant.rs` (`StoredValue`, `reserve_for_row`), `variant_array_builder.rs` (`append_shared`, ≥50% retention in `build`) | `shred_variant::tests::residual_encoding_reuses_storage_with_bounded_capacity` (new): RED 8,337,090 B for 2,098,336 input bytes, GREEN < 1.25× input, mostly-typed < 2.5×, logical parity, shared-row and validity matrix (RED on sharing for row 0); the three named object tests and all 546 package lib tests; learned-layout journey; redux 1002/1002 | PASS |
| REQ-003/010/011; INV-004/006/008; AC-002/008/009 (scenario 6) | `capture.rs::row_bytes`, `scribe_outbox.rs::frames/chunks` | `scribe_outbox::tests::large_captures_reach_bounded_frames_without_a_split_copy` (new): RED 2,897,489 B for 1,155,960 content bytes, GREEN 1,224,313 B in 5 frames, each within budget, retained ≤ wire + wire/4 + 4096, exact readback, subjects, positions, and identical retry and peer ids; the five named outbox tests and capture test; peer, outage and cancellation journeys | PASS |
| REQ-010; AC-008 (scenario 7) | `clients/mod.rs::read_bounded_bytes` | `clients::bounded_answers::oversized_answers_fail_and_refusals_keep_status_and_body` extended: known 100,000 B → capacity == len (RED 114,602), chunked, unrepresentable and oversized advertised lengths, truncated body; `native_transport_reuses_owned_buffers` | PASS |
| REQ-012; AC-003/010; INV-008 | — | paired profiles, full L1 on both sides, rule reruns, main Gateway | see below; payload c64 peak RSS unresolved |

Non-goals stayed excluded: no batching or concurrency retune, no server ingress
change, no public, generated or durable contract change, no new dependency or
feature. No codegen change.

### AC-010 allocation, paired `--profile` (KiB per reconciled call)

Runs: baseline `20261108T151913Z` and `20261108T154139Z`; candidate
`20261108T153201Z` and `20261108T153536Z`, all VALID. Same method as TASK-003
(Δ`alloc_space` over consecutive steps divided by Δ succeeded
`gateway.buffered`/`gateway.stream.end`). A first baseline attempt
(`20261108T153521Z`) went INVALID with `host.busy` from another session's
build and was rerun.

| Step | Baseline runs 1 / 2 | Candidate runs 1 / 2 | Δ |
|---|---|---|---|
| buffered-64kib-capture-payload.c64 | 2639 / 2637 | 1874 / 1859 | −29% |
| buffered-64kib-capture-payload.c1 | 2883 / 2898 | 2134 / 2220 | −25% |
| buffered-64kib-capture-metadata.c64 | 1154 / 1156 | 764 / 770 | −34% |
| buffered-64kib-capture-metadata.c1 | 1173 / 1175 | 869 / 793 | −29% |
| buffered-1kib-capture-metadata.c64 | 210 / 211 | 202 / 203 | −4% |

Each side repeats to within 10%, and every 64 KiB reduction is larger than that.

Inclusive `alloc_space` stacks over payload c64 minus the preceding c1
interval. Each is an inclusive parent, so they do not add up. Baseline covers
107,733 calls and candidate 159,944, so totals are not per call.

| Stack | Baseline GiB | Candidate GiB |
|---|---|---|
| `token_bound` | 19.77 | 0.06 |
| native `to_raw_value` / `to_json` | 26.42 | 9.73 |
| `to_string<Value>` | 20.36 | 0.91 |
| outbox `take_record_batch` | 13.36 | 0 |
| ingress `shred_variant` | 53.86 | 40.55 |
| `concat` | 13.21 | 19.81 |
| outbox `encode` | 17.61 | 9.29 |
| `typed_body` | 6.72 | 9.94 |
| WAL `compress_to_vec` | 13.52 | 20.00 |

Sizing materialization and the split copy are gone. Native serialization and
residual growth fell (per call, 26.42/107,733 → 9.73/159,944 and
53.86/107,733 → 40.55/159,944). One `concat` remains: the per-group projection
in `CallCapture::project` that concatenates one-row Variant columns. Scenario 6
removed only the second copy. `typed_body` (scenario 3) and the WAL zstd grow
with call count.

### Full unprofiled L1, candidate vs baseline `aa2a6a0d8`

Records (gitignored `benchmarks/history`):
- baseline `*-aa2a6a0d8.json`: writes-parquet `20261108T154622Z`, hot
  `155343Z`, parquet `155830Z`, verification `160319Z`, gateway `160434Z`;
- candidate `*-bea7f8b4c.json`: `161044Z`, `161805Z`, `162150Z`, `162638Z`,
  `162753Z`.

All five experiments ran their three repetitions on both sides. Errors, lost,
shed and undrained are 0 on every repetition. Bifrost (all three) and Gateway
are VALID on both sides. Verification is INVALID on both sides only for
`driver.saturated` at c64: the driver used 6.43–6.55 cores against a 6.40
limit. Every Verification criterion is met, and each Verification median is
inside the baseline range (c64 p99 0.368 vs 0.350–0.389 ms, RSS 3106 vs
2629–3417 MiB). Hot and parquet reads show no headline metric (rate, p99, peak
RSS) more than 5% worse than the baseline range. Parquet c1 peak RSS sits
3–5% above the baseline maximum (for example `span-trace-c1` 1973 vs
1808–1884 MiB), while hot RSS is below it at every step.

Gateway, candidate median vs baseline range:

| Step | Rate req/s | CPU µs/req | p99 ms | Peak RSS MiB |
|---|---|---|---|---|
| buffered-1kib-capture-metadata.c1 | 4925 vs 5773–5858 | 248 vs 210–214 | 0.340 vs 0.245–0.256 | 717 vs 608–925 |
| buffered-1kib-capture-metadata.c64 | 55073 vs 55675–57605 | 351 vs 346–364 | 1.455 vs 1.368–1.418 | 2196 vs 2021–2178 |
| streamed-1kib-capture-metadata.c64 | 1537 vs 1538 | 931 vs 939–984 | 0.791 vs 0.490–0.560 | 1375 vs 1648–1875 |
| buffered-64kib-capture-metadata.c1 | 2595 vs 3305–3382 | 441 vs 342–355 | 0.609 vs 0.366–0.406 | 1934 vs 1240–1504 |
| buffered-64kib-capture-metadata.c64 | 23333 vs 20834–22065 | 838 vs 877–913 | 3.224 vs 3.357–3.721 | 2212 vs 2133–2196 |
| buffered-64kib-capture-payload.c1 | 2414 vs 2437–2563 | 710 vs 689–725 | 0.673 vs 0.536–0.654 | 2271 vs 2462–2543 |
| buffered-64kib-capture-payload.c64 | 8728 vs 5521–5793 | 1675 vs 1715–1753 | 56 vs 135–147 | 5085 vs 4606–4716 |

Writes-parquet flagged `otlp-spans-c64`: rate 510 vs 532–768, p99 230 vs
177–218 ms, RSS 3231 vs 3018–3079 MiB. It also flagged `otlp-logs-c64` RSS
(4923 vs 3988–4469 MiB, with rate 563 vs 441–552) and `native-write-c1` p99
(81 vs 28–59 ms).

**Rule reruns, one fresh back-to-back pair per engine** (target records):
- Gateway: baseline `20261108T170624Z`, candidate `20261108T171236Z`.
  - Cleared: `buffered-1kib-capture-metadata.c1` 5337 vs 5012–5831 req/s and
    233 vs 211–248 µs; `buffered-64kib-capture-metadata.c1` 3418 vs 3030–3356
    req/s and 340 vs 358–459 µs; `streamed-1kib-capture-metadata.c64` p99
    0.509 vs 0.468–0.505 ms (+0.7%).
  - Payload c64 still exceeds the baseline in peak RSS: 5533 vs
    4047–4439 MiB, with rate 8967 vs 5734–5827, CPU 1752 vs 1694–1707 µs
    (+3%) and p99 53 vs 107–126 ms.
- Writes: baseline `20261108T171531Z`, candidate `20261108T172651Z`.
  - Cleared: `otlp-spans-c64` (955 vs 915–953 rows/s, p99 104 vs 115–182 ms,
    RSS 2710 vs 2926–2997 MiB); `otlp-logs-c64` RSS (3980 vs 4114–4723 MiB);
    `native-write-c1` p99 (120 vs 39–123 ms).
  - `otlp-logs-c64` rate is 440 vs 461–692. It was 563 vs 441–552 in the full
    run, so this step's spread across runs is larger than the difference. Its
    single rerun did not clear it, and it is reported as unresolved.

**Payload c64 peak RSS (unresolved).** Each repetition is a fixed 5 s window.
The candidate completes about 55% more calls in it (43–46k vs 28–29k) and
ingests 4.6–5.0 GiB of capture against 2.7–2.8 GiB, with drain 0.3–0.5 s vs
0.75–0.96 s. Peak RSS per captured GiB is lower (0.97–1.22 vs 1.39–1.70 GiB),
but absolute peak RSS exceeds the immediate baseline in every pair. Residual
sharing is not the cause. A worktree with the fork at `a11acb8` (scenario 4
only; record diff
`ddbf272d5f37d3b628ae4ddef823d21c649ef8e7fd456ec11bea82c717cd0e91`) ran back to
back with the candidate:
- Gateway payload c64: no-share `20261108T174300Z` 5123–5500 MiB at
  9.2k req/s, candidate `20261108T174533Z` 4859–5060 MiB at 9.1k req/s.
- Writes `otlp-spans-c64`: no-share `20261108T175318Z` 2634–3054 MiB, candidate
  `20261108T175509Z` 2552–3041 MiB.

No equal-rate comparison exists, because the harness drives closed-loop clients
without a rate cap. The extra RSS has not been proven to come from the higher
ingest rate.

**AC-003 vs main `5234961d5`** (Gateway `20261108T180612Z`, VALID, `dirty:
false`). Every 64 KiB step beats main's range on rate, CPU and p99. Payload peak
RSS is 5085 vs 7051–7385 MiB at c64 and 2271 vs 4471–4816 MiB at c1. Four
medians fell outside main's range in the worse direction:
- `buffered-1kib-capture-metadata.c1` p99 0.340 vs 0.306–0.316 ms;
- `streamed-1kib-capture-metadata.c1` p99 0.952 vs 0.666–0.900 ms;
- `streamed-1kib-capture-metadata.c64` p99 0.791 vs 0.498–0.612 ms;
- `buffered-64kib-capture-metadata.c1` rate 2595 vs 2941–3160 req/s and p99
  0.609 vs 0.437–0.459 ms.

`buffered-64kib-capture-metadata.c64` RSS is 2212 vs 1761–2160 MiB (+2.4%,
within 5%). The rule's fresh back-to-back pair (main `20261108T181100Z`,
candidate `20261108T181353Z`, both VALID) clears all four:
- 1 KiB c1: 5952 vs 5760–5872 req/s, p99 0.250 vs 0.247–0.252 ms;
- streamed c1: p99 0.766 vs 0.720–0.880 ms;
- streamed c64: p99 0.518 vs 0.501–0.557 ms;
- 64 KiB metadata c1: 3429 vs 3148–3204 req/s, p99 0.380 vs 0.400–0.436 ms.

### Review decision evidence

The results above came from runs that overlapped other host load. Everything
in this section was rerun with a whole-host idle gate: below 1.5 busy cores for
60 s before each run. Driver commits on the candidate:
- `ccc65683f`, `93e948642`: `--pace`, the arrivals-free tail and per-outcome
  error counts;
- `5508cf29f`: the driver runtime has `DRIVER_CORES` workers.

Every baseline (`aa2a6a0d8`, `1a0699e42`, main `5234961d5`) ran the identical
driver. For `aa2a6a0d8` and `1a0699e42` the six driver files were copied
byte-identical. Main got only the `5508cf29f` worker patch: the other driver
changes add `--pace` and observations that default L1 never executes.

**Matched-load memory, `--pace 4969` on
`buffered-64kib-capture-payload.c64`.** 4969 req/s is 90% of the lowest valid
baseline sustained rate, with at most 64 calls in flight. Every repetition is
VALID with zero errors, missed or capped arrivals, lost, shed or undrained work.

| Run | Peak RSS MiB | CPU µs/req | p99 ms |
|---|---|---|---|
| B `222017Z` | 2720 / 2631 / 2681 | 911 / 903 / 894 | 0.98 / 0.99 / 0.94 |
| C `222209Z` | 2207 / 2301 / 2250 | 791 / 787 / 797 | 0.86 / 0.89 / 0.93 |
| C `222400Z` | 2090 / 2220 / 2153 | 780 / 781 / 779 | — |
| B `222551Z` | 2502 / 2463 / 2525 | 894–907 | — |
| C `224422Z` | 2236 / 2217 / 2194 | 776 / 783 / 781 | 0.83 / 0.88 / 0.83 |
| B `224614Z` | 2482 / 2516 / 2665 | 877 / 899 / 877 | 0.92 / 0.98 / 0.98 |

- **Peak RSS** is below the baseline minimum in every candidate repetition.
- **Opening and post-drain RSS** are lower on the candidate. In the last pair,
  opening is 1626–1867 vs 1830–2203 MiB and post-drain resident is 1812–2020 vs
  1830–2363 MiB.
- **Governed ownership by owner**, last pair, post-drain: `oracle_used` 1024 MiB,
  `forge_used` and `transport_used` 0, `scribe_used` 811/863/934 vs
  790/789/765 MiB. That is far inside the 91 GiB managed plan.
- **Post-drain settling.** The candidate's post-drain `scribe_used` rose across
  cycles in each pair, which is a sampling artifact. Polling `/metrics` every
  2 s during the last pair shows the same bounded sawtooth on both sides.
  - Range: about 750 to 2040 MiB, made of the active memtable (at most about
    512 MiB at rotation) plus 3–6 immutable generations.
  - Charge: both sides charge 44 KiB per inserted row (`memtable_bytes_inserted`
    over `memtable_rows_inserted`).
  - Each post-drain reading is one point on that cycle, and every candidate
    reading lies within it. Ownership does not grow beyond rotation.
- **Profiled pair** (`222742Z` B, INVALID with `{"capped": 18}` under profiler
  overhead and used for attribution only; `222934Z` C VALID):
  - allocation is 2742 vs 1979 KiB per completed call;
  - live heap at the end is 878 vs 1082 MiB, at least 92% of it
    `shred_variant` output held by the memtable (810 vs 992 MiB). That fits the
    same sawtooth sampled at a different phase.

**Bifrost `otlp-logs-c64` (unsettled).** Two predetermined pairs in the order
B, C, C, B, then two more runs.

| Run | Rate req/s | CPU µs/row | p99 ms |
|---|---|---|---|
| B `223126Z` | 824 / 740 / 737 | 27.1 / 26.1 / 25.6 | 142 / 183 / 186 |
| C `223317Z` | 754 / 731 / 695 | 25.3 / 25.4 / 25.2 | 160 / 211 / 236 |
| C `223509Z` | 739 / 501 / 507 | 27.5 / 23.2 / 23.9 | 204 / 292 / 260 |
| B `223700Z` | 685 / 667 / 660 | 24.5 / 24.6 / 24.5 | 183 / 184 / 208 |
| final full L1 B `231100Z` / C `233230Z` | C median 560 vs B 329–420 | 28 vs 24–28 | 221 vs 274–649 |
| rerun B `003319Z` / C `003745Z` | C median 430 vs B 607–758 | 40 vs 27–39 | 355 vs 158–185 |

- **Slow repetitions are fsync-bound.** At about 500 req/s the server uses about
  6 of 10 cores: `phase_wal_sync` p50 33–40 ms vs 19–25 ms,
  `phase_scribe` p50 80–88 vs 50–57 ms, and fewer WAL syncs (98–99 vs
  149–153).
- **Not a CPU cost.** WAL bytes per row are equal (433–435 vs 431–434), and CPU
  per row is lower in those repetitions.
- **No candidate change here.** Nothing in the candidate changes the WAL or the
  logs encoding path, apart from the Variant shredder.
- **Direction flips between runs:** candidate faster in the full L1, slower in
  the rerun. The finding remains unsettled and is not fixed.

**Verification driver.** The cost was the driver's Tokio runtime: it ran one
worker per host CPU (32) against a declared 8-core budget.
- **Profile** (`perf`, `direct-verify-c64`, before the fix): the SDK client
  stack accounts for 35% of driver CPU, response decoding 19% (the response is
  the public contract), hyper connection tasks 10%, and scheduler park/steal 6%
  plus kernel futex time.
- **Fix** (`5508cf29f`): the driver runtime is sized from the single
  `DRIVER_CORES` constant that also feeds `Validity::driver_cores`.
- **Result:** with the same workload and the same 6.40-core threshold, the
  driver uses 5.99–6.14 cores at 82–87k verifications/s, against 6.43–6.62
  cores at 76–80k/s before. That is about 72 vs 82 µs per call.
- **Records:** both final full-L1 records are VALID (B `232656Z`,
  C `234827Z`). The earlier INVALID records stay INVALID. The margin is about
  4%.

**Final full L1, idle-gated.** Baseline `aa2a6a0d8` records are
`20261108T231100Z`, `231821Z`, `232206Z`, `232656Z` and `232811Z`; candidate
`5508cf29f` records are `233230Z`, `233954Z`, `234338Z`, `234827Z` and
`234942Z`. All ten are VALID with zero errors, lost, shed or undrained work.

Gateway, candidate median vs immediate baseline range:

| Step | Rate req/s | CPU µs/req | p99 ms | Peak RSS MiB |
|---|---|---|---|---|
| buffered-1kib-capture-metadata.c1 | 5936 vs 5812–6030 | 210 vs 204–212 | 0.261 vs 0.242–0.245 | 769 vs 596–923 |
| buffered-1kib-capture-metadata.c64 | 59386 vs 56994–59237 | 345 vs 341–364 | 1.347 vs 1.339–1.413 | 2202 vs 2089–2213 |
| streamed-1kib-capture-metadata.c64 | 1539 vs 1537–1539 | 957 vs 930–1023 | 0.558 vs 0.523–0.596 | 1746 vs 1661–1895 |
| buffered-64kib-capture-metadata.c1 | 3361 vs 3340–3458 | 349 vs 342–402 | 0.397 vs 0.382–0.424 | 1340 vs 1371–1486 |
| buffered-64kib-capture-metadata.c64 | 24831 vs 22940–23634 | 836 vs 877–914 | 3.105 vs 3.199–3.437 | 2174 vs 2194–2258 |
| buffered-64kib-capture-payload.c1 | 2703 vs 2565–2598 | 660 vs 676–695 | 0.508 vs 0.531–0.545 | 2199 vs 2401–2596 |
| buffered-64kib-capture-payload.c64 | 9271 vs 5724–5825 | 1652 vs 1749–1802 | 58 vs 138–144 | 5353 vs 4619–4663 |

- **Payload c64 peak RSS** is above the immediate baseline at the candidate's
  60% higher closed-loop rate. It is below it at matched load (above).
- **1 KiB c1 p99** was rerun with a fresh pair (B `004213Z`, C `004405Z`):
  0.2665 vs 0.2596–0.2624 ms (+1.6%).

**AC-010 against the pre-TASK-002 baseline `1a0699e42` (passes).**
- **Unprofiled** (`20261109T001558Z`): payload c64
  - peak RSS 5353 vs 7289–7694 MiB, below the minimum repetition;
  - rate 9271 vs 4733–4850 req/s;
  - CPU 1652 vs 2246–2290 µs;
  - p99 58 vs 116–181 ms.

  Payload c1 RSS is 2199 vs 4145–4749 MiB. Every other step is within or better
  than its range.
- **Paired `--profile`** (`1a0699e42` `002438Z`, candidate `002859Z`), KiB per
  reconciled call:

  | Step | `1a0699e42` | Candidate |
  |---|---|---|
  | payload c64 | 4676 | 1856 |
  | payload c1 | 4384 | 2178 |
  | 64 KiB metadata c64 | 1476 | 762 |
  | 1 KiB metadata c64 | 220 | 205 |

**AC-003 against main `5234961d5` (passes, `20261109T002018Z`).**
- **64 KiB steps:** every one beats main's range on rate, CPU and p99. Payload
  c64 peak RSS is 5353 vs 7454–7636 MiB, and c1 is 2199 vs 4378–4598 MiB.
- **1 KiB steps:** 1 KiB c1 p99 is 0.261 vs 0.248–0.271 ms, and streamed c64
  p99 is 0.558 vs 0.491–0.657 ms.
- **Outside main's range in the worse direction:** streamed c1 CPU is 5124 vs
  4959–5041 µs (+1.6%). That step runs at 24 req/s and is idle-dominated.

**Other immediate-baseline differences, each with one fresh rerun pair:**
- **`native-write-c64` (resolved: within the baseline range, no code cause).**
  The full L1 and the first rerun pair were both run baseline first, and the
  candidate ran 11–60% faster in each. Writes rerun B `003319Z`, C `003745Z`:

  | Run | Rate req/s | CPU µs/row | Peak RSS MiB |
  |---|---|---|---|
  | full L1 | 129 vs 110–133 | 2.37 vs 1.97–2.10 | 5704 vs 4508–5160 |
  | rerun | 190 vs 110–125 | 2.52 vs 1.95–2.07 | 6150 vs 4853–5267 |

  - **No candidate change runs on this path.** The fixture
    (`events_ingest`) has six Int64/Utf8 columns and no Variant column.
    The Arrow pin `ca5feff` → `394d4b0` changes only Variant shredding and
    its residual builder, and `bea7f8b4c` changes nothing in `Cargo.lock`
    except that revision. The `scribe_outbox` chunking is unchanged for
    uniform audit rows, and the other server changes are on the Gateway and
    skald paths.
  - **Profiled pair** (B `022911Z`, C `023258Z`, both VALID with complete
    captures). The c64 heap profiles have no `shred_variant` or
    `reserve_for_row` frames. Variant-named frames allocate 0.09 vs 0.10 GiB
    of 312 vs 329 GiB, and all of it is schema-level
    (`enforce_dynamic_variants`). The candidate allocates 5% more bytes for
    13% more requests. Under the profiler, c64 is 153 vs 136 req/s,
    2.17 vs 2.52 µs/row and 6625 vs 6664 MiB.
  - **Order-balanced rerun, B, C, C, B** (B `023801Z`, C `024246Z`,
    C `024703Z`, B `025308Z`). All four runs are VALID with zero errors.
    Each cell is the candidate median of 6 repetitions, with the baseline
    min–max of 6 in brackets:

    | Step | Rate req/s | p99 ms | CPU µs/row | Peak RSS MiB |
    |---|---|---|---|---|
    | native-write, 1 client | 55 (49–65) | 144 (52–214) | 1.51 (1.48–1.65) | 1720 (1641–1996) |
    | native-write, 64 clients | 146 (108–146) | 1148 (1083–2252) | 2.29 (1.99–2.60) | 5183 (4675–6292) |

    - 64-client CPU per row: the median is 10% above the baseline median
      (2.09) at an 11% higher rate. The baseline's fastest repetition
      (146 req/s) is also its most expensive (2.60 µs/row, 6292 MiB), so
      cost per row rises with the closed-loop rate on both sides.
    - Nothing is outside the baseline range.

  No Arrow fork change was made, because the `reserve_exact` residual
  hypothesis has no allocation on this path to act on.
- **`native-write-c1` p99:** 226 vs 78–193 ms in the full L1, cleared on rerun
  (73 vs 98–200 ms).
- **`otlp-spans-1k-c64` rate:** 225 vs 250–256 in the full L1, cleared on rerun
  (412 vs 408–418).
- **Parquet c1 reads** (rerun B `004556Z`, C `005003Z`). TASK-004 does not
  change the read path.
  - RSS is 2–7% above the baseline maximum in both runs (for example
    `span-trace-c1` 1316 vs 1222–1292 MiB).
  - `point-c1` is 491 vs 513–521 req/s, with p99 2.94 vs 2.70–2.79 ms.

**Memory acceptance status:**
- matched-load peak RSS: better;
- governed ownership: within budgets;
- post-drain ownership: bounded by rotation on both sides;
- AC-010 against `1a0699e42`: passes;
- Gateway against main: passes.

Open before review: the unsettled `otlp-logs-c64` finding. TASK-004 is not
marked implemented.

Archived records and profiles (gitignored):
- `target/benchmarks-baseline/final` (`aa2a6a0d8`);
- `target/benchmarks-pre002` (`1a0699e42`);
- `target/benchmarks-main` (`5234961d5`);
- `benchmarks/history` and `target/benchmarks` (candidate).

### Commands

```sh
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^components::gateway::routes::usage_bound_tests::/) or test(/^components::gateway::capture::tests::/) or test(/^scribe_outbox::tests::/)'   # 35/35
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib          # 67/67
mise exec -- cargo nextest run --locked -p wyrd-skald-providers --lib  # 49/49
cargo nextest run -p bohmian-parquet-variant-compute --lib             # fork, 546/546; clippy clean in touched files
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(=write_read::learned_layout_evolution_preserves_published_evidence)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=peer::oracle_only_gateway_captures_through_the_peer_scribe)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=compatible::openai_compatible_streams_terminate_and_survive_a_capture_outage)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=resilience::a_cancelled_call_is_still_settled_and_the_server_keeps_serving)"'
mise run test:bifrost:integration:redux                                # 1002/1002
mise run fmt:check && mise run lints && mise run check:deps && mise run check:tenant-isolation && mise run check:workspace-hack
mise exec -- git diff --check
mise run bench:l1:gateway -- --allow-dirty --profile                   # ×2 per side
mise run bench:l1 -- --allow-dirty                                     # baseline and candidate
mise run bench:l1:gateway -- --allow-dirty                             # main
mise run bench:l1:gateway -- --allow-dirty --steps <step>              # rule rerun pair
mise run bench:l1:bifrost:writes -- --allow-dirty --steps <step>       # rule rerun pair
mise run bench:l1:bifrost:writes -- --allow-dirty --profile --steps native-write-c1,native-write-c64   # B then C, idle-gated
mise run bench:l1:bifrost:writes -- --allow-dirty --steps native-write-c1,native-write-c64             # B, C, C, B, idle-gated
mise run bench:l1:gateway -- --allow-dirty --steps buffered-64kib-capture-payload.c64 --pace 4969   # matched load, B/C alternating
mise run bench:l1:bifrost:writes -- --allow-dirty --steps otlp-logs-c64                         # B, C, C, B
scripts/postgres/with-test-postgres.sh -- perf record -F 499 --call-graph fp -- target/profiling/release/capacity l1 verification --allow-dirty --steps direct-verify-c64
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --lib --bin capacity'   # 116/116
```

Baseline, main and no-share artifacts (records, timings, profiles) are in `target/benchmarks-baseline` and
`target/benchmarks-noshare`. Candidate profiles are in `target/benchmarks`.
