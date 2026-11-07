---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-resource-scaling
spec_revision: 4
requirements: [REQ-008, REQ-010, REQ-011, REQ-012, INV-005, INV-006, INV-008, AC-003, AC-007, AC-008, AC-009, AC-010]
depends_on: [TASK-002]
---

# Remove native transport copies and sampled-value rebuilding

## Outcome and Value

Reduce allocation further on top of TASK-002 without retaining larger buffers or
changing what a request, capture or freshly learned layout means. This task owns
the two next demonstrated causes in the native payload-capture path: HTTP body
copying/duplicate response decoding, and constructing complete Variant payloads
solely to update sampling counters. Finish with a full L1 rerun.

Today a native request is serialized once but its bytes are copied into transport
and deep-cloned even for the first attempt. A native response is decoded for usage,
discarded, then decoded again for capture. A selected shredded sample is rebuilt
into a complete encoded Variant before the sampler counts keys and types. After
this task, transport shares one immutable serialized body, capture consumes the
existing usage decode, and sampling reads selected typed and residual values
without constructing replacement payload arrays.

## What Changes and What Must Stay True

This is the caller-requested follow-on decomposition of approved revision 4's
existing allocation requirements. The spec's reference to TASK-002 as its single
added task describes the initial plan, not a prohibition on this additional task.
No requirement, acceptance threshold or public/persisted contract is replaced.

Extend the existing Skald provider transport, Gateway completion/capture and
Iceberg Variant sampler. REQ-008 allows complete-value reconstruction only after
sample selection; this task removes that intermediate representation entirely
while preserving the same observations. It does not change the sampling policy.

The requested fork work is a compatible update of the already-pinned Iceberg fork
within the same dependency family/version line, not a new dependency or feature.
Here the spec's dependency exclusion continues to prohibit adding crate dependency
edges, families or features; it does not turn an implementation fix in an already
pinned fork into a new dependency. Preserve the existing resolved family/version
line and dependency boundaries, and record the changed immutable SHAs.
Add a narrowly required borrowed traversal primitive to the existing Arrow fork
only if the sampler cannot access complete selected values through its current
typed/residual representation. No replacement Variant implementation, sampler,
parser, dependency family, major version, or local-path production override is
authorized. Update the Bifrost description of selected-observation access to
describe complete logical inspection rather than mandatory materialization via
`unshred_variants`; leave its policy and durability rules intact.
"Reconstruct only selected observations" remains the upper bound on permitted
reconstruction, not a requirement to allocate reconstructed observations when a
complete borrowed traversal gives identical results.

Native request serialization still rewrites the deployment model. Transfer the
serialized allocation into the existing transport owner and let retries share
immutable backing; do not deep-clone it for the first attempt or each retry.
Update the native OpenAI, Anthropic, Google and Vertex raw-call consumers and
ordinary provider RawV1/JSON transport callers together. Keep existing provider
request/response enums, external SDK/wire shapes, validation, auth headers,
redirect policy, timeouts, retry count/backoff and exchange timing semantics.
Ownership plumbing between Rust runtime owners may change; no parallel transport
or new public product API is needed.

Transfer valid UTF-8 buffered response bytes into text without copying. Preserve
the current lossy replacement behavior for invalid UTF-8, body-size checks before
status handling, exact native JSON response text, refusal bodies and stable errors.
After usage extraction, move the decoded native response into selected capture;
scrub only that capture value. Keep caller bytes unchanged. Do not retain decoded
trees when capture is unselected, or change encoded-credential rejection.

Every claim still samples freshly with the identical seed, chosen positions,
strata, weights, thresholds, depth/child bounds and list-element counting. Inspect
selected typed leaves together with their residuals, counting each logical field
once and preserving null/missing/type-conflict semantics. Sampling remains
counters-only, with bounded traversal state and existing reservation ownership.
Avoid a selected-row `take` that copies large payloads as well as the subsequent
unshred. Whole-value output and incompatible merge conversion remain valid users
of reconstruction outside sampling. Cache learning, retry output bytes and Forge
footer-weighted combination are unchanged.

Exclude ingress residual encoder rewrites, changing `shrink_to_fit`, speculative
capacity hints, WAL/zstd scratch reuse, pools, caches, allocator changes, translated
request preparation, media capture, outbox geometry/batching and read-path tuning.
These are separate opportunities, not fallback scope if the selected work is hard.
No weakening of existing memory reservations, ACK/recovery or accepted-row progress
is allowed. Existing AC-003 failures and the long-tail read regression are not
waived or silently relabelled by this task.

## Approach

1. Preserve immutable baseline `d608ee303083d4217949ccd232372818c9c98efa` and
   record current fork pins and fixed resource geometry. Use its implementation
   as the dependency; do not claim TASK-002's open review/performance proof passed.
2. Complete transport ownership and all raw/JSON call sites, including retry and
   buffered response conversion, using the existing provider owners.
3. Carry the existing native usage decode into selected capture and remove the
   second decode without changing returned raw bytes.
4. Extend the existing fork sampler to inspect selected physical/residual content
   directly. Integrate a reproducible immutable compatible fork pin and the owner
   prose/rustdoc changes; preserve sampling equivalence and governance.
5. Run the minimal focused correctness checks below and establish corrected
   allocation-profile population accounting. Reprofile to verify removed stacks.
6. End with the full unprofiled L1 benchmark on baseline and candidate, report
   allocation/memory/performance changes and retain any unresolved failures.

## Ordered Implementation Scenarios

### Scenario 1 — Native transport retains one body through retries

**Behavior.** Native/raw and ordinary JSON calls send the same serialized bytes
and deployment model on the first attempt and retries using shared immutable
backing. Success/refusal text remains exact; valid UTF-8 conversion transfers its
buffer and invalid UTF-8 preserves replacement behavior. Oversized answers still
fail before status processing. Cancellation and terminal failure release ownership.

**RED.** Add one planned, table-driven
`clients::bounded_answers::native_transport_reuses_owned_buffers` in
`wyrd-skald-providers`. Exercise real mock HTTP attempts and focused ownership
evidence for first send/retry, plus valid/invalid UTF-8 and refusal/bound cases.
The current path deep-clones each request and always allocates text on response.
Run [transport proof](#transport-proof); confirm the selector before RED. Extend
existing fixtures rather than add another transport harness.

**GREEN.** Complete ownership through all current callers without changing wire
behavior or retry policy. Keep serialization once and make backing lifetime cover
the actual exchanges, not just the initial async waiter. Rerun this scenario and
the existing bound/raw regressions.

**REFACTOR.** Consolidate the owned-body/text path in the existing provider owner;
delete obsolete copies. Keep request body construction separate from policy.

### Scenario 2 — Selected native capture consumes the usage decode

**Behavior.** Decode native response JSON once for usage. When capture is selected,
move that decoded value into the existing scrubber; when unselected, release it.
Caller bytes, usage, capture selection, credential scrubbing and failure behavior
stay unchanged, including escaped/non-ASCII strings and credential echoes.

**RED.** Extend existing
`adapter::tests::selected_capture_is_scrubbed_bounded_and_leaves_answers_unchanged`
with decode-work evidence for selected/unselected native answers and assertions on
usage/raw bytes. The current successful selected response decodes twice. Run
[capture proof](#capture-proof). This is an extension of one existing regression,
not a new test suite or production telemetry contract.

**GREEN.** Complete response ownership through completion and capture while
preserving translated and streaming behavior. Rerun scenarios 1–2 and native byte
preservation. Do not optimize translated usage extraction in this task.

**REFACTOR.** Remove the redundant native decode and unused retained tree, using
the existing `ResponseCapture` owner rather than another capture representation.

### Scenario 3 — Fresh sampling needs no reconstructed payload arrays

**Behavior.** Given identical logical input, the direct physical/residual traversal
selects the same observations and layout as the current reconstruction-based
sampler. Cover canonical, shredded and mixed batches, typed/residual overlap,
residual-only newly eligible fields, null/missing/type conflicts and nested/list
values. Changing payload string length without changing keys/types must not cause
sampling to allocate replacement payload bytes proportional to that length.
Malformed physical/residual storage retains the existing validation errors rather
than silently producing partial counters or a different layout.

**RED.** Add one planned table-driven fork test
`writer::file_writer::variant_shredding::tests::physical_sample_counts_without_payload_reconstruction`
in the existing `iceberg` library test module. Compare layouts/counts/sample
positions against complete logical input and assert absence of payload-building
work, with deterministic evidence rather than a flaky process-RSS assertion.
Current `ColumnSample::values` takes/copies selected arrays and unshreds them.
Run [sampler proof](#sampler-proof), confirming the actual selector before RED.
Retain and extend the existing shredded/logical parity case instead of duplicating
its fixture inventory.

**GREEN.** Extend the existing analyzer/Variant traversal and repin the existing
forks coherently. Read complete selected values without encoding them, keeping
counter/traversal ownership bounded and policy/results unchanged. Run this test,
fork parity, and the focused Wyrd mixed-layout/lifecycle regressions below.

**REFACTOR.** Remove sampling-only selected-array construction/unshred once parity
holds. Reuse the existing analyzer and counters; do not duplicate a traversal or
introduce a second sampler in Wyrd.

## Acceptance Criteria

| Spec obligations | Required observation and proof |
|---|---|
| REQ-010; INV-008; AC-008 | Scenarios 1–2 remove first-send/retry deep copies, valid-text copying and duplicate native decode. Wire text, bounds, errors, usage and scrubbed captures remain exact. |
| REQ-008; INV-005; AC-007 | Scenario 3 proves fresh selection/count/layout parity including residual discovery, with no selected payload-array rebuilding. A cached layout never bypasses sampling. |
| REQ-011; INV-006; AC-009 | Existing governed mixed-layout/publication journey remains green. No new uncharged buffer/cache or loss of accepted data/progress is introduced. |
| REQ-012; INV-008; AC-010 | Matched profile intervals show removed stacks and allocation/call reduction beyond repeat variation versus d608ee303, without increased retained ownership or peak RSS beyond baseline variation. Whole-change AC-010 remains measured against its original pre-TASK-002 baseline. |
| AC-003; preserved AC-006 | Final full L1 reports every engine/step, retaining main-based non-regression and imported read obligations. Valid exits alone do not pass; prior failures remain failures unless resolved under their existing rule. |

## Implementation Reference

Source-backed reuse evidence, inspected at d608ee303; paths are guidance, not a
private file/type allowlist.

- `skald-providers/src/raw.rs::send_raw` copies borrowed raw text into a Vec.
  `clients/mod.rs::send_bytes_with_retry` deep-clones it for every exchange and
  uses `from_utf8_lossy(...).into_owned()` on all buffered answers. Existing
  `Payload` already owns an HTTP body. Extend these owners; update native client
  methods in `clients/{openai,anthropic,google,vertex}.rs`, ProviderClient RawV1
  consumers and `send_json_with_retry` together. Package name is
  **`wyrd-skald-providers`**, although its library/dependency alias is
  `skald_providers`/`skald-providers`. Reqwest is already installed; no new body
  dependency is justified solely for ownership conversion.
- `wyrd-gateway/src/adapter/http.rs::Native::send` serializes `NativeBody` once;
  preserve that model-substituting serializer. `adapter/mod.rs::complete` decodes
  RawV1 for usage and drops the tree; `http.rs::captured` decodes again.
  `engine.rs::ResponseCapture::json` already consumes and scrubs an owned Value.
  Extend this completion-to-capture workflow, not routing or a new parser.
- Existing Iceberg pin is `2902554d1ea0b5d1d41dd8f45e2a02c83c0e56e7`, Arrow pin
  `ca5feff3b1c523640a03acaa437096b8934c3749`. In Iceberg
  `crates/iceberg/src/writer/file_writer/variant_shredding.rs`,
  `VariantSampler::offer` chooses positions, `VariantLayoutAnalyzer::observe`
  updates counters, and `ColumnSample::values` takes selected physical rows then
  unshreds them. Extend that analyzer to inspect the existing representation.
  Arrow's `VariantArray::try_value` is not a complete merged typed/residual object
  view; swapping to it alone is incorrect. A borrowed visitor, if needed, belongs
  to the existing fork owner rather than Wyrd. Keep existing semantic validation.
- Wyrd `scribe/claim_sample.rs::ClaimVariantSample::layout` already offers native
  physical batches with stable global positions and uses byte-shaped readers.
  `scribe/parquet_writer.rs` owns residual-discovery, retry and physical reuse
  regressions. Keep these consumers on the existing sampler; do not restore whole
  batch reconstruction or change merge reservations to fit the optimization.
- The ~79.8 GiB `append_slice` profile bucket is Variant encoding: ~38.8 GiB
  ingress residual construction, ~27.8 GiB selected-sample unshred, ~13.1 GiB
  capture serialization. This task targets the sample portion, not all 79.8 GiB.
  Variant `build` already transfers ownership after a possible shrink allocation.
  Do not claim transport plus sampling removes all encoder costs.
- `capacity/gateway/run.rs::Overhead::step` discards warmup readings but the heap
  profiler is cumulative across warmup and measured work. Existing profile
  snapshots and capture/timing counters own the measurement fix. Align allocation
  deltas with the complete reconciled-call population; do not divide them by only
  measured repetition calls. No new profiler or benchmark framework is needed.

## Verification and Evidence

Keep tests minimal: two new focused cases (transport and sampler), extend one
capture regression, and reuse two existing integration/journey regressions. Do not
add tests for trivial ownership plumbing already covered by these cases. If new
nontrivial profile-accounting logic is necessary, extend its existing owner's test
with a warmup/measured/drain interval case and record its exact selector before RED;
static population reconciliation needs no manufactured RED.

All new names are **planned**. Packages, modules, targets and existing selectors
were confirmed from manifests/source. Confirm actual selectors after adding tests,
before RED. Run Cargo commands sequentially; use actual package names, not aliases.

### Transport proof

`mise exec -- cargo nextest run --locked -p wyrd-skald-providers --lib -E 'test(=clients::bounded_answers::native_transport_reuses_owned_buffers)'`

Existing bound and RawV1 regressions:

```sh
mise exec -- cargo nextest run --locked -p wyrd-skald-providers --lib -E 'test(=clients::bounded_answers::oversized_answers_fail_and_refusals_keep_status_and_body)'
mise exec -- cargo nextest run --locked -p wyrd-skald-providers --lib -E 'test(=raw::raw_passthrough::raw_v1_writes_body_bytes_directly_and_returns_raw_response)'
```

### Capture proof

```sh
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=adapter::tests::selected_capture_is_scrubbed_bounded_and_leaves_answers_unchanged)'
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=adapter::tests::native_answer_bytes_are_preserved)'
```

### Sampler proof

Use an isolated writable checkout based on the pinned Iceberg source, not Cargo's
cache or the unrelated local checkout HEAD `0991fef0a`. Set
`WYRD_ICEBERG_WORKTREE` to its recorded absolute root. The inspected manifest owns
package `iceberg`, its library target and this module with default features; no
service credentials are needed. Read its AGENTS.md before editing.

```sh
mise exec -- cargo nextest run --locked --manifest-path "$WYRD_ICEBERG_WORKTREE/Cargo.toml" -p iceberg --lib -E 'test(=writer::file_writer::variant_shredding::tests::physical_sample_counts_without_payload_reconstruction)'
mise exec -- cargo nextest run --locked --manifest-path "$WYRD_ICEBERG_WORKTREE/Cargo.toml" -p iceberg --lib -E 'test(=writer::file_writer::variant_shredding::tests::shredded_input_samples_like_its_logical_values)'
```

Record any needed Arrow fork proof at its actual existing owner/selector after
inspection; do not introduce a new Arrow API speculatively. Final Wyrd verification
must resolve immutable canonical fork SHAs through Cargo.toml/Cargo.lock, not a
temporary local patch. Coordinated compatible repins retain one Arrow/Iceberg stack
and existing features. Git commit/push scheduling remains caller-owned.

### Wyrd integration and journey preservation

```sh
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::parquet_writer::tests::mixed_staged_layouts_preserve_values_and_fresh_discovery)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(=write_read::learned_layout_evolution_preserves_published_evidence)"'
```

These reuse real file encoding and the real server/SDK lifecycle. No new language
capability or public contract is introduced, so no duplicate cross-language story
suite is needed in this task. Integrated change review retains its broad gates.

### Owner checks

- `mise exec -- cargo nextest run --locked -p wyrd-skald-providers --lib` — provider
  caller, response bound, error, retry and cancellation closure.
- `mise exec -- cargo nextest run --locked -p wyrd-gateway --lib` — native and
  unchanged translated/streaming capture behavior.
- `mise run test:bifrost:integration:redux` — coherent fork repin and unchanged
  Scribe/reader/Forge compatibility with repository Postgres setup.
- `mise run fmt:check`, `mise run lints`, `mise run check:deps`,
  `mise run check:tenant-isolation`, `mise exec -- git diff --check` — formatting,
  lint and dependency/tenant boundaries. Run the changed fork's own format/lint
  checks through its documented runner; record its exact commands in evidence.

### Allocation evidence, then final L1 rerun

Use d608ee303 as the immediate before/after baseline with identical settings,
builds and host. Use main and pre-TASK-002 baselines separately for the existing
spec criteria; this task does not reset those comparisons.

Before claiming bytes/call, reconcile numerator and denominator over the same
settled profile interval, including warmup, measured calls and drain. Prefer
existing cumulative call/capture snapshots at the profile boundaries; if missing,
add only the profile diagnostic observation needed at the existing owner. Preserve
normal benchmark workloads, thresholds and record compatibility. Do not reuse
4.26 MiB/call as a normalized reference without correcting its population. Run
enough matched profile intervals to distinguish reduction from repeat variation;
label an aggregate honestly rather than calling it a per-repetition median.

1. After focused checks, run paired baseline/candidate
   `mise run bench:l1:gateway -- --allow-dirty --profile`. Report payload c1/c64
   and matched metadata cases; prove the first-attempt/retry copy, second native
   decode and sampled-value unshred/build stacks disappear as applicable. Keep
   live heap, peak RSS, post-drain ownership and capture reconciliation separate
   from cumulative allocation. Use identical profiling on both sides.
2. **End with full unprofiled L1:**
   `mise run bench:l1 -- --allow-dirty` on the baseline and final candidate, on a
   quiet host with no overlapping builds/tests/experiments. This runs all existing
   Bifrost, Verification and Gateway L1 cases, not just the favorable payload step.
   Report every step and all three repetitions. Apply the existing AC-003 rerun
   rule to Gateway/main; do not rerun until a favorable pair appears. Report other
   engines' existing criteria and any regressions without changing their thresholds.

Record immutable source/fork SHAs, dirty diff hashes, actual resource/outbox
geometry, artifact paths, elapsed/drain work, errors/lost/shed/undrained, allocation
population, CPU/rate/p99, RSS and remaining top allocations. Each 64 KiB case has
both request and response bodies of that size. Improvements must not come from
less completed work, larger retained capacities, dropped evidence or altered
sampling/layouts. Absolute allocation floors and additional RSS reductions are
not promised; the original whole-change acceptance remains binding.

## Material Stop Conditions

- Sampling parity requires changing chosen observations, thresholds, weights,
  residual precedence, logical values, retry bytes or persisted layout contracts:
  stop for specification authority rather than accept a different layout policy.
- A new dependency/feature, major version, durable representation, sampler,
  transport framework, cross-tier owner or external SDK/wire change is necessary:
  establish why existing owners cannot satisfy the task and request authority.
- The compatible fork work cannot be integrated under immutable canonical pins:
  record the unavailable Git/dependency authority; do not ship Cargo cache edits
  or a local-path override as complete work.
- Measurement cannot reconcile warmup/drain population, or work cannot remain
  byte-governed and make progress after ACK: finish unaffected work, report the
  exact gap and do not claim success. Do not raise budgets to hide a failure.
- A confirmed existing gate failure remains: preserve its FAIL status and follow
  the normal recovery/authority rules. This task grants no performance waiver.

Private ownership types, traversal organization, equivalent test selectors and
small fixture extensions remain implementation-owned decisions.

## Authority Links

- [Approved spec, revision 4](../spec.md)
- [TASK-002 implementation and evidence](TASK-002-payload-allocation-and-layout-reuse.md)
- [Repository rules](../../../../AGENTS.md)
- [Spec-driven development](../../../spec-driven-development.md)
- [Implementation execution](../../../implementation-execution.md)
- [Testing](../../../../TESTING.md)
- [Bifrost sampling, memory and durability](../../../../architecture/bifrost-design.md)
- [Security posture](../../../../architecture/wyrd-security-posture.md)
- [Benchmark method](../../../../benchmarks/README.md)
