# Gateway boundary review

Candidate: `5c980841140721cfaa1945fba1057675805ca77a`; base: `0ae00f4293f3d2dcf636bcbd88a419fe6245c6dd`. Source read from detached `/tmp/wyrd-task012-review-5c9808411`; actual cumulative diff inspected. Original checkout CodeGraph was used first for navigation; immutable source was checked directly. No implementation changed and no Cargo/benchmark command ran.

## Authority and scope

Reviewed revision 16 amendment, its finalized shared-workload design, task `012-R1-allocation.md` scenarios 1/4/5/6/7, root AGENTS, Rust and maintainer guidance, testing and spec-driven workflow, owning `architecture/wyrd-design.md` principal/capture and gateway quota sections, and `architecture/wyrd-security-posture.md` gateway subject/actor/capture boundary. No nested AGENTS applies to the changed gateway paths. Revision 16 supersedes historical speculative precharge contracts; none are reinstated here.

This review owns gateway request/body retention, answer/stream/capture retention, nested tenant/workload attribution, authorization/commercial limits, and participant lifetime. It does not judge scheduler internals, Monty worker lifecycle, or Bifrost execution independently.

## Coverage and evidence

| Promise / boundary | Traced path and assertions | Assessment |
|---|---|---|
| REQ-019 initiating identity / REQ-023 local permission and quota preservation | `ServerWyrdGatewayCaller::invoke` constructs nested Verification invocation; `admit_call` derives tenant from verified Caller, performs snapshot/decide/route/attribution before workload admission. `WorkloadManager::admit` joins enclosing operation scope. Commercial/provider ledger and capture reservation remain existing owners. `critical_pressure_cancels_the_execution_a_model_call_belongs_to` asserts zero Gateway attribution and cancellation of enclosing execution. | Source/assertions support correct identity and preserved authority. Execution binding limits below. |
| REQ-019 request buffers, REQ-023 active lifetime | Body-limit middleware offers a single `BodyCustody`; ingress clock transfers it to invocation; `_body_custody` attributed to admitted operation persists through CallTask. Tests `a_taken_body_charge_outlives_the_response`, `call_bodies_count_against_the_calling_workload_while_the_call_runs` check handoff, exact standalone/nested attribution, final drop. | Supported for traced request-buffer path. |
| REQ-021/023 pressure refusal and deadlines | `admit_call` waits on manager before commercial reservation, under fixed timeout. Call cancellation child token passed into operation and engine. Existing calls use tracked CallTask; stream settles on end; cancellation/deadline/error classification unchanged. Tests cover critical pause/recovery, running standalone victim, nested execution victim, buffered memory refusal. | Source/assertions support these paths; CPU omissions independent below. |
| REQ-019/023 queued stream frames | `Frame::split` charges Vec capacity once, stores charge in `Bytes::from_owner`, slices zero-copy, relays through bounded channel; `events_body` transfers charged Bytes to transport. Error frame is a bounded mandatory exception; ordinary refused frame ends with existing capacity error. Adapter stalled-reader test holds first frame after stream completion and checks remaining charge; real HTTP slow-reader journey checks rise/bound/completion/disconnect. | Source/assertions support queued frame custody, cancellation and refusal. The test transport chunks are bounded; this is not proof that arbitrary shared allocations equal 64 KiB each. Root capacity charge still bounds larger allocation retention. |
| Capture refusal and bounded protocol state | Stream/MediaCapture drop accumulated content and charge when a new charge fails. Usage member count bounded to 32; translated Gemini index constrained to requested choices. Adapter tests assert refusal, bounded member count and candidate failure. | Refusal paths supported. Successful capture transfer fails GW-02. |
| Buffered response ownership | `AdmittedCall::custody` creates attributed charge; `GatewayCallResponse` carries it and HTTP relay retains it in body closure. Tests cover returned response and body lifetime. | Fails GW-03 at allocation/consumer boundary. |
| REQ-022/AC-017 gateway CPU work | Tracked CallTask runs gateway engine directly; adapter prepare/complete, response credential capture, stream decoder/translation/framing and capture projection run synchronously without CpuLanes. | Fails GW-01. |
| Added/moved waits | New wait is shared process workload admission in `admit_call`, before capture/commercial reservation and without CPU lease. Existing snapshot/auth, provider/Vault IO, stream caller backpressure, object persistence and outbox reservation waits retain their existing owners; no new gateway SQL statement or durable queue appears. | No new prohibited persistence wait found. No CPU lease across IO because gateway acquires none, which does not satisfy required CPU dispatch. |

Available task evidence includes gateway lib/middleware regressions, gateway native journeys, connected isolated-capacity and slow-stream journeys. Assertions were read, not inferred from test names. Parent reports background `test:gateway:native` exit 0 (17 Rust plus 8 Python), but original worktree had an uncommitted Oracle fix at review start, so that is not independently bound to this immutable candidate. No fresh execution is claimed here. Historical tests first written after production remain TDD violations; mutation evidence proves sensitivity, not test-first history.

## Proposed blocking findings

### GW-01 — MISSING: substantial gateway CPU phases bypass shared CPU admission

**Authority:** revision 16 REQ-022 integrates substantial encoding/decoding; AC-017 spans engines and nested calls. Finalized design `shared-workload-management.md:196–198` explicitly includes substantial gateway encoding/decoding. Task scenarios do not waive this; task line 527's statement that gateway calls take no lane describes the omission.

**Locations:** `wyrd-server/src/components/gateway/invocation.rs:587–591,1409–1427` tracked task and direct engine execute; `wyrd-gateway/src/adapter/http.rs:685–696,763–776` prepare/complete/capture; `adapter/stream.rs:838–899,933–982` decode/translate/frame work. `ProviderAttempt` carries only ByteCharge, no lane dispatcher/context. Gateway source has no managed CPU submission or surrounding lane wrapper.

**Supported scenario / trigger:** With all C shared lanes occupied by verification or Bifrost work, a standalone request requiring a substantial cross-provider translation or response decode, or an already-open stream delivering substantial JSON events, reaches the adapter. These supported operations continue their CPU work directly on Tokio even though gateway is demanding CPU service and no lane is free. A nested verification gateway call is spawned onto CallTask, so a caller's task-local context/lease cannot control it or the separately spawned stream relay.

**Expected / actual / consequence:** Gateway CPU phases should queue and run with initiating workload/tenant context, releasing at provider and downstream waits. Actual phases run outside CpuLanes, bypassing C-lane service selection and allowing standalone gateway decoding/encoding to compete outside the shared scheduling policy. This is an assigned engine integration missing, not an exact-preemption request.

**Smallest correction boundary:** At existing gateway CPU phase owners, pass the shared dispatch/context and use existing CpuLanes for substantial prepare/decode/translation/capture phases and separate relay tasks. Keep HTTP/provider/Vault/channel waits outside held leases, preserve existing deadline/cancellation/queue bounds, and reuse the scheduler rather than introducing another queue. Do not put a lifetime permit around the whole provider call.

**Closure:** Focused gateway/server integration holding all lanes should show a substantial real adapter CPU phase queues, then completes after release with correct tenant/workload; a held network/caller wait leaves a lane free. Cover nested identity and the stream relay's independently spawned execution. Source tracing must cover sibling buffered, translated stream and capture phases. Existing gateway native journeys then prove unchanged wire behavior.

### GW-02 — INCORRECT: successful stream capture drops its charge before retained content is disposed or handed to outbox

**Authority:** REQ-019 substantial retained buffers count through ownership transfer; REQ-023 custody survives until disposal and moves to outbox. Finalized design per-area Gateway row names retained capture buffers.

**Locations:** `wyrd-gateway/src/adapter/stream.rs:189–193` and `252–262`; `engine.rs:283–303,405–417` StreamEnd/ResponseCapture; downstream `wyrd-server/src/components/gateway/invocation.rs:1177–1185,1249–1268,1565–1626`; `components/gateway/capture.rs:724–726,750–806` binary object owners and storage waits.

**Supported scenario / trigger:** Enable payload response capture for a generated speech stream or streamed JSON answer and let the stream terminate successfully with substantial accumulated content. `MediaCapture::finish` moves bytes into ResponseCapture, or StreamCapture moves events, but neither moves `self.charge`. Rust drops that remaining field on return. StreamEnd now owns the complete capture while its accumulated ledger debit has returned. For media, `facts` moves bytes into PayloadObjects; `capture.persist` can hold them across object get/put/read-back IO before outbox stage.

**Expected / actual / consequence:** The same retained content should retain its charge until final disposal or representation handoff. Actual successful finish returns the charge before StreamEnd consumption, capture projection, object persistence, and staging. Workload soft targets/root admission can therefore treat substantial live capture bytes as free; asynchronous storage delays extend the gap. Aggregate allocator pressure is not a replacement for an explicitly substantial owner already charged during streaming. Unlike refusal, success must transfer rather than release custody.

**Evidence strength:** Both finish bodies omit the charge. StreamEnd and ResponseCapture have no custody field/owner. PayloadObjects and CallCapture likewise have no retained-memory charge before `stage`. Existing `a_stream_capture_is_charged_until_memory_refuses_it` and media counterpart only assert refusal drops everything; neither tests successful finish while returned content stays alive.

**Smallest correction boundary:** Keep capture bytes and existing same-ledger charge together through successful stream finish, StreamEnd, facts and object persistence; transfer/reconcile at existing projection/outbox handoff without a second root debit. Preserve capture shedding on refusal and storage failure, redaction, tenant paths and answered-call independence. No estimates of future copies or historical precharge policy.

**Closure:** Owner-level test: finish a successful captured stream, keep returned capture alive, observe charge retained, then dispose and observe release. Connected capture test: hold storage while a substantial media capture persists, observe custody throughout, then permit success/failure and verify final release or single outbox takeover. No production change needed to unrelated capture policy.

### GW-03 — INCORRECT: buffered answer custody follows the HTTP body instead of its retained allocation

**Authority:** REQ-019 says substantial owners use allocated capacity where available and a shared physical buffer counts once until last reference drops; REQ-023 retains actual buffer custody. Applies to public buffered JSON/media answers.

**Locations:** `wyrd-server/src/components/gateway/routes.rs:1494–1516` closure retains sidecar charge, returns original unowned Bytes; `components/gateway/invocation.rs:303–314` charges JSON logical length; `wyrd-gateway/src/engine.rs:92–120` JsonAnswer explicitly preserves String spare capacity. Existing `pg_invocation_tests.rs:6972–6985` consumes media body into `delivered`, keeps delivered Bytes in scope, and asserts zero attributed bytes.

**Supported scenario / trigger:** A transport consumes the Full body's single data frame and then drops the exhausted body while retaining the frame's Bytes to write to a slow connection. Equivalently an owning test uses `BodyExt::frame`/`into_data` then drops the body while keeping data alive. The body closure is destroyed and its charge drops; the substantial payload still exists. This ownership pattern is supported by normal HTTP body consumption, not an invented API. Separately native JsonAnswer explicitly retains read buffer spare capacity, but the sidecar charge counts only text length.

**Expected / actual / consequence:** Retained response allocation, including available capacity, should count once until its last consumer reference drops. Actual debit expires with a wrapper whose lifetime can end before the data. Logical-length charging additionally understates a retained capacity which the owner makes available as String. Streams already solve this with Bytes::from_owner(Charged); buffered HTTP uses a different, incomplete custody mechanism. A slow transport can leave released ledger capacity while its outgoing large buffered response remains retained.

**Smallest correction boundary:** Make buffered response allocation and its attributed charge one owner before HTTP handoff, reusing the installed owner-backed Bytes pattern used for Frame, preserving zero-copy media when possible. Account available JSON owning capacity instead of only logical length. Keep charge across returned Bytes clones/slices and release on last owner disposal. No new generalized allocator or speculative envelope.

**Closure:** Focused response-owner/body test should retain data after dropping/exhausting body, assert nonzero unchanged charge, then clone/slice/drop to prove one physical debit and last-reference release. Include buffered JSON with spare capacity and buffered media. Update existing test's zero-after-collection expectation because collection itself returns retained bytes. Existing returned-response attribution/refusal tests continue to pass.

## Result

Gateway boundary cannot pass while GW-01, GW-02 and GW-03 remain. These are discovery proposals for independent validation; stable FIND IDs belong to the validator. No separate finding is raised for unrelated existing capture architecture or the excluded performance experiments.

Skipped: fresh Cargo/benchmark execution and non-gateway internals, as assigned. Risk: successful background test results are not proof of the immutable release candidate while the build fix is dirty.
