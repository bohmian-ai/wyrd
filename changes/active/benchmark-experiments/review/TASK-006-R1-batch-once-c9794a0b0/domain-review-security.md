# Domain review — security / trust boundary (TASK-006-R1 batch-once, `55f87cef1...c9794a0b0`)

## Reviewed boundary

Which local Scribe deliveries count as canonical ingress (`IngressPayload::Canonical`, which skips
Scribe's built-in value validator), from producer staging through `ScribeIngressFrame` to
`execution_lanes::decode_rows`. The review also covers keeping peer and native IPC fully validated,
the Step 1 `EncodedVariant::validate_bytes` equivalence, and preserving tenant, audit and attribution
semantics.

## Authority and source coverage

- Authority: AGENTS.md; spec INV-001/INV-002 and the REQ-021 addition; task section "Approved
  revision — build batches once..." (Step 1, Step 3, and the fixed decisions "Local Scribe delivery"
  and "Peer/native transport"); `architecture/wyrd-security-posture.md` §outbox (lines ~786-797,
  changed) and `architecture/bifrost-design.md` §audit_log outbox (~1221-1233) plus §ScribeHotStage
  (~712), both changed. The posture text reads "trust follows from the in-process call, never from a
  flag a caller can set", and the code matches it.
- Source traced:
  - `scribe_outbox.rs`: `FramePayload`, `ScribeBatch::into_frame` (:628-673), `ScribeRoute::{select,
    in_process, submit}` (:791-829), `ScribePeers::submit` (:757), `Chunk::encode` (:1222-1258),
    `identify` (:1366), `Part::dense`, `ScribeWrite::rows`.
  - `grpc/capture_peer.rs::batch` (:60-92).
  - `contracts.rs::{IngressPayload, CanonicalIngress::unreserved}`.
  - `scribe/ingress.rs`: request-limit check (~:78), `take_transport_decode_owner`,
    `plan_transport_payload` (~:320), `prepare_admitted_rows` (~:388-420).
  - `scribe/execution_lanes.rs`: `decode` (:423), `decode_rows` (:564-640),
    `enforce_builtin_source_contract` (:962-995).
  - `tables/mod.rs`: `validate_predeclared`, `validate_variant_values` (:526-530).
  - `wyrd-queue/src/variant.rs`: `from_bytes` (:289), `validate_bytes` (:306), `validate`, and the
    `variant/serialize.rs` guards.
  - `tables/staged.rs::{Cell, StagedRow::new, project}`; `tables/audit/projection.rs::stage_audit_event`
    (:61).
  - `verification/results.rs` Variant cells; `gateway/capture.rs::{calls_batch, spans_batch,
    payload_column}`.
  - `gate/attribution.rs::{batch_row_dimensions, batch_card_uids, batch_uids}` against the native
    equivalents.
  - Tests: `scribe_persistence_path.rs::assert_producer_rows_share_the_wal_path` (:431) and
    `local_canonical_and_peer_ipc_preserve_frames_and_replay`.
- CodeGraph is present. Tracing was done directly with git diff and grep over the named symbols.

## Findings by obligation

1. **Only server-owned producers reach `Canonical`.**
   - `FramePayload::Rows` is created at exactly one place, `Chunk::encode` (:1253). That happens only
     when `in_process`, which is `ScribeRoute::Local`. The route is chosen once per sink from the
     process's own dependencies (:791-806).
   - Chunks come only from outbox `ScribeWrite` variants: Audit (`StagedRow`), Result and
     ExecutionInputs (`StagedRow`), Capture (`CallCapture::project`), and ServerSpans
     (`project_spans`).
   - `ScribeBatch` and `FramePayload` are `pub(crate)`, and no other server path builds `Canonical`
     (grep). The other pre-existing `Canonical` producers are Gate (`gate/mod.rs:1005`) and replay,
     and neither is changed.
2. **No external, peer, native or OTLP input or wire field reaches it.**
   - `IngestCaptureRequest` is unchanged, with no proto diff.
   - `capture_peer.rs:90` always builds `FramePayload::Ipc`, and `into_frame` maps that to
     `IngressPayload::ArrowIpc`, so `trusted = false` at `execution_lanes.rs:423`.
   - The native and test-native entry (`scribe/mod.rs:739`) stays `ArrowIpc`.
   - The new test sends a peer IPC stream with corrupt Variant bytes and asserts a validation refusal.
3. **Validation the canonical skip removes is now established by construction at the producer.**
   - `trusted` skips only `canonical_validator` (`execution_lanes.rs:977`). For predeclared tables
     that validator is `validate_predeclared`: undeclared-column refusal, declared-Variant validity,
     and partial-struct refusal.
   - Undeclared columns are still refused by `conform_to_registered`.
   - Variant validity holds by construction:
     - `Cell::Variant` holds only `EncodedVariant`, built via `from_serialize`, which guards depth,
       64-bit range and non-finite values, or else falls back to `from_json_text`.
     - Gateway bodies go through `EncodedVariant::from_json_text` (`payload_column`).
     - Spans use the same `project_resource_spans` projection Gate already submits canonically.
   - Partial structs:
     - Only `gateway.calls.resolved_model` is a whole struct. It is `Option<ModelRef>`, and both
       fields are required, so a partial struct cannot be produced.
     - `StagedRow` has no struct cells.
   - `StagedRow::new` enforces cell kind, field count and non-null fields.
4. **The canonical path keeps its other checks.** `decode_rows` still runs the following for both
   payloads:
   - duplicate-field refusal
   - reserved-column refusal
   - built-in `wyrd_` refusal (this runs before the `trusted` return)
   - registered-schema conformance
   - fingerprint check
   - dynamic-Variant enforcement
   - managed stamping
   - field-id stamping

   Request size is checked from `measured_wire_bytes`. The producer sets this to the real encoded
   stream length (`wire_bytes`) that the batch id hashed, so it is identical to the peer path.
   Canonical batches also pass `plan_canonical`, `validate_decoded_request_size`, and the
   decoded-material limit. `unreserved` means no transport owner, so ordinary root memory admission
   applies. Split frames are copied with `take_record_batch` into their own buffers, so each one is
   charged only for its own rows.
5. **Tenant isolation and attribution are unchanged.**
   - The `ScribeBatch.tenant` and `authenticated_tenant` flow is unchanged. The peer receiver still
     limits the reserved system tenant to audit.
   - Card-scope inputs (`attributed_cards`, dimensions) are now read from the batch by
     `batch_card_uids` and `batch_uids`. These have the same column, the same UTF-8 type check, and
     the same distinct parse as `native_uids`. The test `batch_card_uids_match_the_encoded_stream`
     pins this.
   - Card-scope authorization still runs in `decode_rows` for both payloads.
6. **Audit semantics are unchanged.**
   - Permission decisions and the non-blocking staging behavior are unchanged.
   - Audit detail is still encoded at the producer (`stage_audit_event`). Encoding changed from JCS
     text + `from_json_text` to `from_serialize`, which stores the same logical value; the existing
     test compares it with the canonical JSON.
   - `AuditDetail` is typed and was never redacted at this boundary.
   - The canonical JSON preimage that Forge comparisons use is computed from the typed detail, not
     from stored bytes.
7. **Step 1 is equivalent.**
   - `validate_bytes` is `Self::validate(..).map(drop)`. `from_bytes` is the same `validate` call
     followed by an infallible copy, so it accepts and refuses the same inputs with the same violation
     and the same precedence.
   - Row and column attribution (`into_error(label, row)`) is unchanged at the single call site.
   - P1 covers the valid, malformed, oversized, too-deep, nested and numeric fixtures.

## Verification limits

- No cargo was run, per instruction. I relied on the orchestrator's `verification.md` reruns: P1,
  P2/P3 outbox, the real-WAL P3, and attribution, all passing.
- Lints, docs and tenant-isolation lanes were not rerun and are carried from task evidence.
- I did not re-audit Gate's pre-existing public canonical Arrow validation, which is outside this
  range.

## Proposed findings

None material.

## Minor observations (non-blocking)

- **SEC-OBS-1.** `ServerSpans` (`scribe_outbox.rs` `ScribeWrite::ServerSpans`) also takes the local
  canonical path. The task's eligibility wording names verification, audit and gateway capture. Server
  spans are a server-owned projection built by the same validated `project_resource_spans` that Gate
  already submits canonically. The posture text says "server-projected rows", so this is consistent
  with authority. No action is needed.
- **SEC-OBS-2.** `ScribePeers::submit` panics on `FramePayload::Rows` (:759). The panic cannot be
  reached, because the route is fixed per sink and `encode` prepares each frame for that route. It is
  documented under `# Panics`.

## Result

**PASS**
