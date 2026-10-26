# Task review — behavior (`behavior-rev`)

- Subject: base `55f87cef1`, candidate `c9794a0b0` (`git diff 55f87cef1...c9794a0b0`).
- Acceptance: TASK-006-R1 "Approved revision — build batches once and remove shared Scribe copies"
  (task line 1095, Steps 1–5), REQ-021, AC-014, INV-001/002.
- Method: read-only source and diff review. No cargo, mise, or benchmark runs. Benchmark JSON records
  under `benchmarks/` and smoke records under `target/benchmarks/smoke/` were read, not modified.
- I treated the task's appended execution evidence as claims and checked each against source or records.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Design owners record producer/canonical/persistence behavior | `architecture/bifrost-design.md` (ScribeHotStage gather; outbox dense build, local canonical, peer full validation); `wyrd-security-posture.md` (in-process trust, no caller flag); `spec.md` REQ-021 bullet; no new revision number | doc diff read | PASS (note O-3 on ordering) |
| S1 `EncodedVariant::validate_bytes` delegates to the existing validator; no new scanner, same precedence | `crates/shared/wyrd-queue/src/variant.rs:307` → `Self::validate(..).map(drop)`; `from_bytes` kept | `variant::tests::validation_only_matches_owned_validation` (valid, malformed, bad metadata, oversized, too deep, nested deep, numeric, compared to `from_bytes`) | PASS |
| S1 table validator stops building owned copies | `crates/vala/vala-bifrost-redux/src/tables/mod.rs:529` | existing `tables::tests::builtin_registry_dispatches_canonical_value_validation` (row/column attribution unchanged, since `into_error(label,row)` is unchanged) | PASS |
| S1 GREEN profile/allocation evidence | none recorded | none | FAIL (F-1) |
| S2 verification/audit stage owned scalars and encoded Variants, not one-row batches | `tables/staged.rs` `Cell`/`StagedRow`; `verification/results.rs` `ResultRow`, `variant()`, `time()`; `tables/audit/projection.rs::stage_audit_event` | `tables::staged::tests::staged_rows_project_into_one_dense_batch`; results tests | PASS |
| S2 dense build at drain, grouped by destination/schema, order and attribution kept, no one-row concat | `scribe_outbox.rs` `Part::dense` (all-staged run → `StagedRow::project`), `Chunk::encode`, `Frame::absorb/shares` | `scribe_outbox::tests::dense_projection_preserves_records_and_attribution` (mixed slice, order, nulls, Variants, sub/act pairs, audit carries own pair, capture bulk projection) | PASS |
| S2 Gateway capture keeps `CallCapture::project`; native/OTLP path unchanged | `components/gateway/capture.rs` changes only the recording test double; capture enters as `Part::Batch` | dense test reference = `CallCapture::project` | PASS |
| S2 weights charge retained values; item/byte/reserved limits unchanged | `ScribeSink::weight` → `StagedRow::owned_bytes`; no constant changed in diff | `staged_writes_charge_their_owned_memory`; drift large-output charge kept | PASS |
| S2 lane key uses retained identity, same semantics | `ScribeSink::lane` → `StagedRow::text(..)`; `group_activity` same | existing lane tests (RV1/RV2 claimed) | PASS |
| S2 removed tests mapped to retained properties | mapping lists 2 of 3 removed tests | task evidence line 1512 | FAIL (F-2, minor) |
| S2 GREEN "report actual allocations/copying and encoded geometry" | none recorded | none | FAIL (F-1) |
| S3 local route delivers `IngressPayload::Canonical` via `CanonicalIngress::unreserved`; ordinary admission | `FramePayload::Rows`, `ScribeBatch::into_frame`, `ScribeRoute::in_process` | `local_canonical_and_peer_ipc_preserve_frames_and_replay` asserts `canonical == in_process` at the payload seam | PASS |
| S3 identity and split from IPC on both routes; lost-ACK replay | `Chunk::encode` always encodes and hashes IPC, then swaps in rows (`take_record_batch` for split ranges) | same test: identical `batch_id → (table, rows, ipc)` maps across routes, lost ACK replays | PASS |
| S3 peer/native IPC keeps full validation; no wire trust flag | `grpc/capture_peer.rs` builds `FramePayload::Ipc`; proto unchanged; `ScribePeers::submit` accepts only `Ipc` | `scribe_persistence_path::canonical_nested_batches_share_one_managed_wal_path` + `assert_producer_rows_share_the_wal_path` (corrupt Variant IPC refused, not as `IngestBusy`) | PASS |
| S3 attribution/dimensions read from in-process rows the same way | `gate/attribution.rs` `batch_row_dimensions`, `batch_card_uids` | `batch_card_uids_match_the_encoded_stream` | PASS |
| S3 RED "forced pressure" and GREEN "retained memory for local and peer separately" | existing `a_retry_resubmits_identical_frames` (local `IngestBusy`) now runs the canonical route; no new memory check | no retained-memory observation per route | FAIL (part of F-1) |
| S4 key-only concat, comparator kept, gather payload with `interleave_record_batch`, no whole-payload concat | `scribe/parquet_writer.rs::prepare_sorted_candidate`; `sort_keys` kept; `sort_batch`/`concat_batches`/`take` removed from production | `gathered_sort_matches_concatenated_reference` (4 sources, ties, nulls, desc/nulls-first, hour boundary, Variant + list, 5 ranges, explicit keys); `sort_batch_uses_one_key_for_every_table` reworked | PASS |
| S4 key/position/output memory budgeted through the existing persistence owner | one `try_reserve_maintenance(Persistence, keys+positions+unshared_arrow_bytes(sources))` before building; `append_charged_batch` unchanged | `gathered_sort_respects_persistence_admission` (refused as `IngestBusy`, relieved, no leaked bytes) | PASS |
| S4 candidates, preflight, merge, WAL/fence unchanged | no other hunks in Scribe durability paths | claimed `scribe::` 379/379, `cross_shard` + `source_boundary` journeys | PASS (claimed runs not re-executed) |
| S4 GREEN "confirm copy/allocation reductions without growing merge work" | none recorded | none | FAIL (F-1) |
| Fixed decisions: 1 shard, pools/lanes/frames/outbox limits, fail-fast admission, no linger/sleep, 3 tables, no new durable table | no constant, config, mise, or benchmark-source change in the diff | `git diff` grep of constants/config: empty | PASS |
| Test change `pg_verification_groups::Support::snapshot` | `flush_bifrost()` before reading the group | diagnosis checked below | PASS |
| Rustdoc on new/changed items (AGENTS.md gate) | almost every item documented; two items can panic and have no `# Panics` | source read | FAIL (F-3, minor) |
| Ponytail ladder on new machinery | `StagedRow`/`Cell` is the smallest shared owner for two producers over four tables; `FramePayload` and `batch_*` helpers are needed to avoid re-encode/decode | — | PASS (observation O-1) |
| S5 / P5 all L1 criteria (verification c64 p99 < 500 µs, Gateway, Bifrost) | — | clean records on `f50fdd9fb`: all five INVALID | FAIL (F-4) |
| S5 separate diagnostic evidence per intervention | none recorded | none | FAIL (F-1) |
| Stop condition invoked correctly | evidence reports red, changes no criterion/driver/topology, asks for maintainer direction | records read | PASS |

## Diagnosis check — `pg_verification_groups::available_rows_form_expiring_group_snapshots`

The recorded cause is credible and the fix is in the right place.

- `Support::execution` polls `GET /v1/verification/executions/{id}` until it returns 200. That
  status comes from the PostgreSQL queue row (`components/verification/service.rs::get_execution`).
  `state.group` is filled only from the execution's Bifrost input record (`records.record(..)`), and
  is `None` while that record is unpublished.
- `architecture/wyrd-design.md:1676` says the group's source selection is returned "once its input
  record is visible", so `group: null` before publication is the documented public behavior. A test
  that equates "readable" with "group visible" is racy against the documented contract.
- The fix is in the shared helper `Support::snapshot`, so all three callers (lines 616, 679, 686)
  get it. The other explicit read at line 1113 asserts null on purpose and is not affected.
  Production read semantics are unchanged.
- Not verifiable read-only: the claim that it reproduced on HEAD before this revision. No separate
  diagnostician artifact exists. The diagnosis is recorded in the task evidence (symptom, evidence,
  cause, fix site), which meets the recorded-diagnosis requirement.

## Step 5 assessment

The records confirm the claims. Each record is clean (`dirty: false`, commit `f50fdd9fb`) and each
verdict is INVALID:

- **Verification.** Both prepasses failed `driver.saturated` (6.96 and 6.74 cores, over 6.40). The
  smoke records support the stated cause. Server CPU per verification fell from about 371 µs
  (`415082a72` smoke PASS) to about 271 µs, more requests completed, and smoke c64 overhead p99 fell
  from 0.99 ms to 0.74 ms. The 0.74 ms p99 is still above the 0.5 ms criterion, so verification
  would not pass even with a different driver budget.
- **Gateway.** `chat-capture.c64` p99 is about 4.5 ms, with about 86% of outbox items shed.
  `chat.c64` median p99 is 0.517 ms, and repetition 1 shed 2,208 capture items. `chat.c64` shedding
  and a p99 above 0.5 ms were already present at `2840dfbf6` (p99 0.62–0.65 ms, up to 16,935 shed),
  so this is an existing failure that improved, not a regression. The evidence table lists only
  `300c60a1d` as the prior record, and that record has no `chat.c64` step. The earlier comparison is
  in `20261026T080639Z-2840dfbf6`.
- **Bifrost.** writes-parquet ran 311.8 s against a 300 s budget, hot point-c64 p99 is 8.0 ms, and
  parquet point-c64 p99 is 13.1 ms. All were red before this revision. The query paths are untouched.

What this means for acceptance: Step 5 is a required acceptance step ("A remaining red criterion
keeps this task incomplete"), so the task cannot PASS. The stop condition was used correctly. No
criterion, driver budget, topology, pool, admission, or workload was changed. The evidence says the
task is incomplete and asks for maintainer direction, which is the required response. Changing the
verification driver budget or any Gateway/Bifrost criterion is a benchmark-criterion decision. Only
the maintainer can authorize it, so I do not count it as an implementer omission.

## Proposed findings

### F-1 — MISSING: per-intervention allocation/copy/geometry diagnostic evidence

- **Violated obligation:** Step 1 GREEN (task line 1199: "Confirm profile/allocation evidence no
  longer attributes discarded owned buffer construction"). Step 2 GREEN (line 1249: "Report actual
  allocations/copying and encoded geometry, not only producer elapsed time"). Step 3 GREEN (line
  1297: "Check retained memory for local and peer attempts separately"). Step 4 GREEN ("confirm
  copy/allocation reductions without growing downstream merge work"). Step 5 (line 1348: "Retain
  separate diagnostic evidence for each intervention"). P5 ("rows/frame, decoded batch geometry,
  allocations/copied bytes"). Acceptance mapping REQ-021, REQ-019, and AC-010/014.
- **Location:** `changes/active/benchmark-experiments/tasks/TASK-006-R1-simplify-verification-data-plumbing.md`
  "Approved revision execution evidence — Steps 1–5" (lines 1496–1559).
- **Evidence:** The Step 1–4 rows mark PASS on unit/integration selectors alone. There is no
  profile, allocation count, copied-byte figure, rows-per-frame, or per-route retained-memory
  observation for any step. The only performance data is the integrated P5 run, plus smoke CPU per
  verification, which is quoted only to explain the driver guard.
- **Consequence:** REQ-021 and AC-014 require the record to tell real CPU savings apart from work
  transfer, changed load, and measurement artifacts, step by step. Without per-step evidence the
  maintainer cannot tell which of Steps 1–4 produced the roughly 27% CPU-per-verification drop. They
  also cannot tell whether the local canonical path moved memory into Scribe or grew merge work. That
  is the information they need for the direction the evidence asks for.
- **Materiality:** These are explicit GREEN obligations in the approved task and acceptance mapping,
  not optional hardening. The Step 1–4 PASS rows overstate completion.
- **Testable correction:** Add a bounded diagnostic subsection with one profiled or allocation-counted
  run per step, using existing benchmark/profiling owners and no new permanent metrics. It should
  report the S1 validation copy share, S2 producer allocations and rows per frame, S3 local versus
  peer retained bytes and the absence of decode, and S4 copied bytes and merge work. Alternatively,
  mark the Step 1–4 GREEN rows incomplete. Low burden: diagnostics only, no code.

### F-2 — MISSING (minor): one removed test absent from the replacement map

- **Violated obligation:** "Link every removed/replaced test to its retained property" and Step 2
  REFACTOR ("Map them to dense-schema, allocation, pressure and replay proof").
- **Location:** `crates/vala/vala-bifrost-redux/src/tables/audit/projection.rs` (deleted
  `text_columns_hold_about_their_values`). Map at task line 1512.
- **Evidence:** The map covers `a_one_row_text_column_holds_about_its_value` and
  `shared_buffer_charges_preserve_owned_memory` only. The map also says
  `staged_rows_project_into_one_dense_batch` "asserts exactly sized builder capacity". It actually
  asserts `< 256` bytes (`staged.rs` test). The new `staged_writes_charge_their_owned_memory` checks
  `weight == Σ owned_bytes`, which follows directly from how `weight` is defined.
- **Consequence:** Only the record is affected. The property, staged audit text holding about its
  value length, still holds structurally through `Box<str>` and is partly covered by the staged test.
- **Materiality:** This is an explicit record obligation, but nothing breaks. Non-blocking alone.
- **Correction:** Add the third mapping and correct the "exactly sized" wording in the evidence.

### F-3 — VIOLATION (minor): changed items that can panic lack `# Panics`

- **Violated obligation:** AGENTS.md "Every new or changed Rust item has rustdoc … `# Panics` when it
  can panic. Missing rustdoc blocks merge."
- **Location:**
  - `crates/vala/vala-bifrost-redux/src/tables/staged.rs:171` `StagedRow::column`, which indexes
    `rows[0]` and `cells[index]`.
  - `crates/wyrd/wyrd-server/src/verification/results.rs:699` test helper `project`, which calls
    `.expect("row projects")`.
- **Evidence:** Neither doc comment has a `# Panics` section. Sibling helpers added in the same
  change, such as the audit projection test `project` and `staged`, do have one.
- **Consequence:** Fails a mandatory repository documentation gate. No runtime effect: `column` is
  only reached after `project` rejects an empty `rows`.
- **Correction:** Add a `# Panics` section to each, naming the invariant (non-empty rows, cells
  checked by `new`) and the failed projection. Two short doc additions.

### F-4 — INCORRECT (acceptance): Step 5 L1 criteria not met

- **Violated obligation:** Step 5 Behavior/GREEN. The one-shard production configuration must pass
  the verification, Gateway, and Bifrost L1 criteria, with verification c64 median p99 under 500 µs.
- **Location:** `benchmarks/verification/direct-verify/20261026T215213Z-f50fdd9fb.json`,
  `…215339Z-f50fdd9fb.json`, `benchmarks/gateway/overhead/20261026T215452Z-f50fdd9fb.json`,
  `benchmarks/bifrost/{writes-parquet,hot,parquet}/20261026T22*-f50fdd9fb.json`.
- **Evidence:** All five verdicts are INVALID. Red criteria are listed in the Step 5 assessment.
  Smoke verification c64 overhead p99 is 0.74 ms.
- **Consequence:** The task is incomplete under its own terms. The implementer reported this
  accurately and invoked the stop condition.
- **Materiality:** This is the revision's acceptance criterion.
- **Correction:** None the implementer can make within the current approval without changing
  topology, pool, criterion, or driver, which is forbidden. It needs maintainer direction on the
  verification driver budget and the remaining Gateway/Bifrost criteria, then a clean P5 rerun.

## Non-blocking observations

- **O-1 (Ponytail).** In `Part::dense`, the mixed `Row`+`Batch` arm projects each staged row as a
  one-row batch and concatenates (`scribe_outbox.rs:295`). Frames group by table, and staged rows
  (audit and verification tables) never share a table with projected batches (calls, spans), so
  that arm is unreachable. The multi-`Batch` concat is still needed. The doc line "only a mix is
  concatenated" (`:271`) also leaves out multiple projected batches. A one-line simplification is
  possible; there is no behavioral consequence.
- **O-2.** In-process split frames copy their row range with `take_record_batch`
  (`Chunk::encode`), so Scribe charges each frame only its own rows. That is a deliberate, documented
  extra copy compared with a zero-copy slice. It is consistent with "do not retain an uncharged
  duplicate payload". `invariant-rev` should confirm the charging.
- **O-3.** Commit order: Step 1 code (`b47082554`) landed three minutes before the design-owner commit
  (`cd26d66d3`), while the task says to record behavior "before executable changes". The final
  integrated state has both, and Step 1 changes no observable behavior.
- **O-4.** Chunk sizing now samples `StagedRow::owned_bytes` instead of one-row Arrow memory. The
  per-row estimate falls sharply, so chunks hold more rows before `Chunk::split` enforces the encoded
  frame budget. Encoded frame bounds and determinism are preserved. Frame geometry changed, and that
  is exactly the rows-per-frame data F-1 asks to be recorded.

## Overall result

**FAIL.** Steps 1–4 are implemented as specified, and I found no correctness, trust-boundary,
identity, or fixed-decision defect in them. The task fails because Step 5 acceptance is red (F-4,
correctly reported, with the stop condition correctly invoked), and because the required
per-intervention diagnostic evidence is missing (F-1). F-2 and F-3 are minor record and rustdoc
corrections.
