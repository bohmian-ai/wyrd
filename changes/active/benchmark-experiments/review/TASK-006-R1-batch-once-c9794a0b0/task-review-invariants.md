# TASK-006-R1 batch-once revision — invariant review (`invariant-rev`)

- Subject: base `55f87cef1`, candidate `c9794a0b0`, `git diff 55f87cef1...c9794a0b0`.
- Scope: producer → outbox → Scribe state and value flow (staging, `Part::dense`,
  `FramePayload::{Ipc,Rows}`, memory weights, frame splitting/identity, lost-ACK replay,
  lane keys, attribution, phase ordering, canonical admission). Persistence sorting
  (Step 4) and P5 benchmarks are outside this scope.
- Method: source reading only (no cargo/mise). Task evidence was treated as claims;
  `verification.md` was not present when this review ran.

## Traced paths

1. **Verification.** `ResultPayloadBuilder::{build,inputs}` → `assemble` → `StagedRow::new`
   (results.rs) → `ScribeWrite::{Result,ExecutionInputs}` → `ScribeSink::lane`/`weight`
   (scribe_outbox.rs:1521/1546) → `ScribeSink::write` (1587) → `frames` → `Frame::absorb`
   (grouped by table and schema pointer) → `chunks` → `Chunk::encode` (1221): `Part::dense`
   (278) → `attributed` → `split` (IPC, halving) → `identify` (1347, SHA-256 over domain,
   tenant, table and IPC) → in process: `FramePayload::Rows` (whole batch, or a
   `take_record_batch` copy of the split range) / peer: `FramePayload::Ipc` →
   `ScribeRoute::submit` → `into_frame` (627) → `IngressPayload::Canonical(unreserved)` |
   `ArrowIpc`.
2. **Audit.** `AuditEvent` → `stage_audit_event` (projection.rs) → `ScribeWrite::Audit(StagedRow)`
   → same frame path, with no appended pair (the row carries its own sub/act/request cells).
3. **Gateway capture.** `ScribeWrite::Capture` → `CallCapture::project` in bulk (unchanged)
   → `Part::Batch` → same encode path; `attributed` still appends `wyrd_request_id` from
   `request_id` for `Calls`.
4. **Peer receiver.** `capture_peer.rs` builds only `FramePayload::Ipc` → `into_frame` →
   `ArrowIpc` → full decode and validation (`execution_lanes.rs:423`, `trusted == false`).
5. **Scribe canonical admission.** `validate_logical_transport_frame` (ingress.rs:66) checks
   `measured_wire_bytes` (the IPC length) against the request ceiling for both payloads →
   `plan_canonical` (expanded/decoded/WAL-bound limits) → `decode_rows`: duplicate fields,
   reserved and carried columns, conform, the `wyrd_*` refusal in
   `enforce_builtin_source_contract`, fingerprint and stamping. Only the value validator is
   skipped (`execution_lanes.rs:977`).

## Acceptance matrix (scope)

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Staged verification/audit rows own scalars and encoded Variants; no one-row Arrow batch per record | `staged.rs` `Cell`/`StagedRow` (`Arc<[Cell]>`, `Box<str>`, `EncodedVariant`); `results.rs::assemble`; `projection.rs::stage_audit_event` | `tables::staged::tests::staged_rows_project_into_one_dense_batch`; results/audit projection tests | PASS |
| Dense build at drain keeps schema, values, nulls, order, grouping and attribution | `Part::dense` (scribe_outbox.rs:278): one `StagedRow::project` for an all-staged run; `attributed` unchanged; owners extended in part order | `dense_projection_preserves_records_and_attribution` (2073): mixed slice, per-destination owners `[0,3]`, `[1,4]`…, sub/act columns per owner, audit width unchanged, capture equal to its bulk projection | PASS |
| Gateway keeps its existing capture projection | `frames` still calls `CallCapture::project(&captures)` with per-call fallback; `Part::Batch` | same test, capture branch; recording double changes only | PASS |
| Retained owned values are charged until settlement/loss; reserved-share, item and byte limits unchanged | `weight` = `StagedRow::owned_bytes` (cells + text bytes + Variant bytes); `SCRIBE_BYTES`, capacity and lane constants untouched; staged rows stay in `items` until `write` returns | `staged_writes_charge_their_owned_memory` (2045) incl. large-output lower bound | PASS (see O-1) |
| No uncharged duplicate payload retained on local routes | Local frames drop the IPC right after `identify`; a split frame carries a compact `take` copy so Scribe charges only its rows; the whole-frame batch is shared by `Arc` with Scribe (`held_material_bytes`); the `Submitted` tuple holds it only until `tally` in the same bounded `deliver` (≤ `FRAMES_IN_FLIGHT`) | code trace; `local_canonical_and_peer_ipc_preserve_frames_and_replay` checks `canonical == in_process` | PASS |
| Retry re-preparation gives identical split frames and ids; lost ACK dedups | `write` rebuilds frames from the same `items` on every attempt; `split` and `identify` run on every route; the payload swap happens only after hashing | `local_canonical_and_peer_ipc_preserve_frames_and_replay` (2949): both routes lose one ACK, `submitted > retained`, same `batch_id → (table, rows, ipc)` map on both routes; `a_retry_resubmits_identical_frames` retained | PASS |
| Local and peer delivery of one prepared frame share one identity, wire size and rows | `FramePayload::Rows { wire_bytes }` = IPC length; `measured_wire_bytes` comes from it; split ranges come from the same `split` output | same test, which compares the re-encoded local rows with the peer IPC | PASS |
| Card scope and dimensions derived equivalently | `batch_card_uids`/`batch_row_dimensions` vs `native_*`: same columns and `parse_distinct` set semantics (order is irrelevant to scope membership) | `gate::attribution::tests::batch_card_uids_match_the_encoded_stream` | PASS |
| Lane keys come from retained typed identity with the same semantics | `lane`: `row.text(WYRD_REQUEST_ID)` / `row.text("execution_id")` on the first row; `SubmissionSink::lane` for queued inputs unchanged; capture `request_id()` unchanged; `StagedRow::text` returns `None` for null or absent, like the old `first` | `a_calls_writes_share_a_lane_and_calls_spread`, `verification_items_share_execution_lanes_and_executions_spread`, `verification_lanes_keep_phase_order_across_retry` (unchanged) | PASS |
| Group activity extraction unchanged | `group_activity` uses `row.text("origin"/"group_id"/WORKLOAD_CARD_UID…)`; correlation cells are part of the staged schema | V6 carried (`pg_verification_groups` 68/68 claimed) | PASS |
| Summaries blocked by retryable detail failure; terminal oversized-output exception unchanged | `write` phases `[details, summaries]` with `?` unchanged; wire-ceiling `PayloadTooLarge` applies to the canonical path via `measured_wire_bytes` (ingress.rs:78); `from_scribe` classification unchanged | existing detail/summary ordering and oversized-output tests in `scribe_outbox::tests` (carried, 580/580 claimed) | PASS |
| Local canonical rows use ordinary admission and managed-envelope checks; peer/native keep full validation | `into_frame` keeps principal, `StampedAttribution::Rows`, request and batch ids; `CanonicalIngress::unreserved` (no transport owner) → ordinary root; peer constructs IPC only; `ScribePeers::submit` refuses `Rows` by an invariant panic | `canonical_nested_batches_share_one_managed_wal_path` extended (dense audit rows canonical and as IPC accepted, stored once each; corrupt Variant IPC refused) | PASS (see O-3) |
| No regression in Gateway/audit request-key routing | capture and audit share the request-id lane; `Calls` `wyrd_request_id` still appended in `attributed` | `a_calls_writes_share_a_lane_and_calls_spread` | PASS |
| Audit detail value preserved | `EncodedVariant::from_serialize(detail)` replaces JCS text + `from_json_text`; `AuditDetail` has no float fields, so only object key insertion order can differ, and the logical value does not | `detail_is_canonical_json`-style assertion via `variant_cell_to_json` | PASS |

## Proposed findings

No blocking findings in this scope.

### Minor observations (non-blocking)

- **O-1 — Variant capacity slack is not charged.** `staged.rs:62` charges
  `EncodedVariant::encoded_bytes()` (`len`, `variant.rs:388`). The retained `Vec<u8>`s come
  from `VariantBuilder::try_finish` without a shrink, so a large staged output can hold up
  to about twice its charged bytes. Before this change the Variant was copied into
  near-exact Arrow buffers, which `get_array_memory_size` charged. Not blocking: the other
  outbox weights in the same function are also length-based (`capture.content_bytes()`,
  `spans.encoded_len()`), the task asks that the payload be charged and it is, and the
  ceiling is a bounded constant factor on Variant bytes only. If the maintainer wants
  heap-exact charging, the one-line fix is to charge `capacity()` through an
  `EncodedVariant` accessor, which costs no copy.
- **O-2 — Unused mixed-part fallback.** `Part::dense` (scribe_outbox.rs:278-300) has a
  mixed `Row`+`Batch` branch that projects one-row batches and concatenates them. This is
  the representation the task removes, and no current producer reaches it: staged
  audit/Verifier schemas never equal capture or span schemas. It is small and gives
  correct output, so it is not material. Returning an error there would remove it.
- **O-3 — Narrow real-WAL coverage.** The real-WAL equivalence extension covers only the
  audit producer shape. Verification and capture canonical shapes rely on the real-server
  lanes (wyrd-server lib, V1/V6/V7) rather than the named P3 Scribe test. No defect was
  found: the admission code path is table-agnostic apart from the definition lookup.

## Verdict

**PASS** for the invariant scope. Values and state stay bounded and attributed from
producer to sink. Frame identity and splitting are route-independent. Lost-ACK replay
reproduces the same ids. Local canonical frames still pass tenant, wire-ceiling, reserved
and carried column, conform, fingerprint, stamping and admission checks.

This verdict does not cover task completion: the task's own evidence records P5 (all L1
criteria) as FAIL, so the task cannot be accepted as complete regardless of this scope.
