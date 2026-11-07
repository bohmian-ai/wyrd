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

## Implementation Evidence

Candidate `aa2a6a0d8` (Wyrd `3ca828260`, `69f58d4f9`, `aa2a6a0d8`) on
baseline `d608ee303`. Forks on `wyrd-production`: iceberg-rust
`e849bc4fa46430e6278a4c201106cdd68dc5b244`, iceberg-compaction
`764fe2a8bbf2a8d1577ca27fec76751ac91c07ff` (repinned to that iceberg). Every
benchmark ran on clean commits (`dirty: false`; the only untracked path is an
unrelated change packet). Host: Ryzen 9 9950X, 32 CPU / 91 GiB, Linux x86_64.

### Reuse map

- **Transport:** `clients::send_bytes_with_retry` builds one
  `reqwest::Request` and sends `try_clone` per attempt. The `Bytes` body is
  shared by refcount. `answer_text` keeps the read buffer when it is valid
  UTF-8. RawV1 takes ownership of `Box<RawValue>` and converts it to bytes with
  no copy.
- **Capture:** `adapter::complete` returns the RawV1 usage decode only when
  capture is selected, and `ResponseCapture::json` consumes it.
- **Sampler:** the fork's `variant_shredding` analyzer reads selected values in
  place through a borrowed typed/residual reader that mirrors
  `unshred_variant`. `ColumnSample::values` no longer takes or unshreds.
  Wyrd's `claim_sample` consumers and reservations are unchanged.
- **Profile population:** no harness change. The last repetition's
  post-drain `TimingEvidence.after` snapshot
  (`timings/<step>/repetition-3.json`) is cumulative per (case, phase),
  warmup included. The heap capture follows it directly. Allocation per call
  is the difference of consecutive steps' `alloc_space`, divided by the
  difference of their `gateway.buffered`/`gateway.stream.end` succeeded counts.
  That counts warmup, measured and drain calls over the same interval. The
  TASK-002 figure of 4.26 MiB/call used measured calls only and is superseded.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-010; INV-008; AC-008 | `skald-providers/src/clients/mod.rs` (`attempt_request`, `answer_text`), `raw.rs::body_bytes`, `wyrd-gateway/src/adapter/{mod,http}.rs` | `clients::bounded_answers::native_transport_reuses_owned_buffers` (RED with the old lossy copy), the `selected_capture_is_scrubbed_bounded_and_leaves_answers_unchanged` extension, providers lib 49/49, gateway lib 67/67 | PASS |
| REQ-008; INV-005; AC-007 | fork `variant_shredding.rs` borrowed reader | `physical_sample_counts_without_payload_reconstruction` (old: 39,824,263 B vs new 50,811 B, every parity assertion equal), iceberg variant tests 53/53 | PASS |
| REQ-011; INV-006; AC-009 | consumers and ×2 reservation unchanged | vala mixed-layouts test, redux tier-2 lane 1002/1002, learned-layout journey | PASS |
| REQ-012; INV-008; AC-010 | removed stacks below | two paired `--profile` intervals; unprofiled L1 and RSS rerun pair | PASS |
| AC-003; preserved AC-006 | — | full L1 baseline and candidate, main Gateway L1, rule reruns | PASS (see below) |

### AC-010 allocation, paired `--profile` (KiB per reconciled call)

Runs: baseline `20261107T211245Z` and `20261107T211927Z`; candidate
`20261107T205853Z` and `20261107T211606Z`, all VALID.

| Step | Baseline runs 1 / 2 | Candidate runs 1 / 2 | Δ |
|---|---|---|---|
| buffered-64kib-capture-payload.c64 | 3310 / 3302 | 2640 / 2644 | −20% |
| buffered-64kib-capture-payload.c1 | 3138 / 3185 | 2804 / 2795 | −11% |
| buffered-64kib-capture-metadata.c64 | 1344 / 1349 | 1152 / 1157 | −14% |
| buffered-64kib-capture-metadata.c1 | 1457 / 1373 | 1174 / 1182 | −17% |
| buffered-1kib-capture-metadata.c64 | 214 / 216 | 212 / 213 | −1% |

Each side repeats to within 6%, so every 64 KiB reduction is larger than the
repeat variation. The streamed 1 KiB steps (about 487 calls at c1) mostly
carry background flush work from adjacent steps, so they are not normalized.

Removed stacks, inclusive `alloc_space` per step, baseline → candidate:

- `unshred_variant`: 36.0 → 0 GiB at payload c64 and 4.4 → 0 GiB at c1.
- The sampler's `assemble_claim`: 40.8 → 1.8 GiB at payload c64.
- `from_str<serde_json::Value>`, the second native decode: 13.1 → 6.6 GiB at
  payload c64.
- The gateway `dispatch` closure: 23.7 → 0 GiB at metadata c64.
- The first-send `send` copy: 1.56 → 0 GiB.

Remaining top leaves at payload c64:
- `append_slice` 48.2 GiB
- `reserve_inner` 24.2 GiB
- `write` 18.2 GiB
- `encode_ipc` 12.7 GiB
- `compress_zstd` 12.5 GiB
- Arrow binary `with_capacity` 12.2 GiB
- `take_bytes` 12.2 GiB
- JSON `end_string` 12.2 GiB

These are ingress Variant encoding, WAL IPC and zstd, and capture
serialization, which are outside this task.

Peak RSS under profiling at payload c64: baseline 5413–5698 MiB, candidate
5260–5662 MiB, overlapping. Cumulative allocation is reported separately from
RSS.

### Full unprofiled L1, candidate vs baseline `d608ee303`

Records (gitignored `benchmarks/history`):
- candidate `*-aa2a6a0d8.json`: writes-parquet `20261107T214316Z`, hot
  `215038Z`, parquet `215420Z`, verification `215908Z`, gateway `220023Z`;
- baseline `*-d608ee303.json`: `212248Z`, `213011Z`, `213354Z`, `213842Z`,
  `213957Z`.

Bifrost (all three) and Gateway are VALID on both sides. Verification is
INVALID on both sides only for `driver.saturated` at c64: the driver used
6.42–6.55 cores against a 6.40 limit. Every Verification criterion is met
(p99 0.335–0.337 ms, zero errors and lost writes). Bifrost and Verification
show no headline metric (rate, p50, p99, peak RSS, CPU per row or verification)
more than 5% outside the baseline range in the worse direction.
`native-write-c64` is 19% faster.

Gateway medians, candidate vs baseline (three repetitions each). Errors,
lost, shed and undrained are 0 on every repetition.

| Step | Rate req/s | CPU µs/req | p99 ms | Peak RSS MiB |
|---|---|---|---|---|
| buffered-64kib-capture-payload.c64 | 5832 vs 5587–5604 | 1749 vs 1841–1937 | 134.9 vs 126.1–145.5 | 4683 vs 4544–4629 |
| buffered-64kib-capture-payload.c1 | 2580 vs 2384–2446 | 692 vs 712–751 | 0.540 vs 0.575–0.579 | 2456 vs 2290–2446 |
| buffered-64kib-capture-metadata.c64 | 21773 vs 19521–20177 | 921 vs 988–1066 | 3.408 vs 3.656–3.891 | 2130 vs 1711–2271 |
| buffered-64kib-capture-metadata.c1 | 3357 vs 3108–3256 | 347 vs 356–378 | 0.381 vs 0.404–0.412 | 1390 vs 1242–1490 |

The payload RSS medians sat just above the baseline maximum (c64 +1.2%,
c1 +0.4%). A fresh back-to-back pair of payload c64 (baseline
`20261107T220908Z`, candidate `20261107T221002Z`) clears it: candidate
4463 MiB vs 4412–4547 MiB, with rate 5963 vs 5384–5821 and CPU 1700 vs
1873–1897 µs.

**AC-003 vs main `5234961d5`** (Gateway `20261107T220348Z`, VALID). Every
candidate median is in range or better except `streamed-1kib-capture-metadata.c1`
CPU: 5207 vs 4959–5041 µs. At 24 req/s that metric moves in steps of about
83 µs, one 10 ms CPU tick per repetition. The rule's fresh back-to-back pair
(main `20261107T220725Z`, candidate `20261107T220816Z`) clears it: candidate
4628 vs 4546–4711 µs. Every 64 KiB step beats main's range on rate, CPU and
p99, and payload RSS is 4683 vs 7155–7652 MiB at c64.

**AC-006 imported reads (hot, p50 median, candidate vs baseline):**
- span-shredded memtable 21.8 vs 22.1 ms;
- span-shredded staged 17.0 vs 16.9 ms;
- span-long-tail staged 37.7 vs 38.1 ms;
- span-long-tail memtable 26.4 vs 26.6 ms.

TASK-002's memtable long-tail regression (26.7 vs 20.2–20.5 ms against
`1a0699e42`) is unchanged and remains reported.

### Commands

```sh
mise exec -- cargo nextest run --locked -p wyrd-skald-providers --lib -E 'test(=clients::bounded_answers::native_transport_reuses_owned_buffers)'
mise exec -- cargo nextest run --locked -p wyrd-skald-providers --lib
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib
cargo test -p iceberg --lib writer::file_writer::variant_shredding   # fork, rustfmt +nightly-2026-03-05
mise run test:bifrost:integration:redux
mise run fmt:check && mise run lints && mise run check:deps && mise run check:tenant-isolation
git diff --check
mise run bench:l1:gateway -- --allow-dirty --profile          # ×2 per side
mise run bench:l1 -- --allow-dirty                            # baseline and candidate
mise run bench:l1:gateway -- --allow-dirty                    # main
mise run bench:l1:gateway -- --allow-dirty --steps <step>     # rule rerun pairs
```

### Non-goals and limits

The following stayed out of scope:
- capacity hints, scratch reuse, pools, caches, allocator changes;
- translated or streamed paths;
- layout policy, thresholds, retry bytes, merge reservations;
- new dependencies or features;
- benchmark harness changes.

No unrelated file changed. The allocation figures are paired aggregates per
step, not per-repetition medians, because the heap profiler captures once per
step. CPU attribution is unavailable because samply needs
`perf_event_paranoid` ≤ 1. Verification's driver saturation is a
driver-budget condition on both sides and was not addressed here.

**Status: IMPLEMENTED.** This routes to `$wyrd-task-review`.
