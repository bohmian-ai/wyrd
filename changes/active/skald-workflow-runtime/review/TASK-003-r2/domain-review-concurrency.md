# Concurrency, cancellation, and context-isolation domain review

**Immutable subject:** base `58d07d7260df1f022a721e720a28ea48e5096e35`, candidate `2e6f13f505daf19050e588ea0d1d7fc936697e27`.

**Overall result: FAIL.** The cumulative implementation is source-correct for request-local model/fallback/deadline/correlation projection, send-once native 401 handling, retained Python client context, remote wait ownership, and ingress fallback isolation. One required cancellation claim lacks the direct recorded proof prescribed by TASK-003.

## Reviewed boundary and authority

I reviewed the approved Revision 12 specification, original TASK-003, the TASK-003-r1 concurrency report and validated findings, and `TASK-003-R1-preserve-client-context-and-native-call-safety.md`, including the human-approved `WyrdGatewayCall.model` amendment and same-spec native-401 task correction. Applicable authority includes `AGENTS.md` async/runtime and test-evidence rules, `architecture/agent-rules.md`, the spec-driven-development and maintainer references, REQ-036A/038/039/043/046/048/058, INV-007/020/021, AC-011A/013/019/020, and the task's prohibition on mutable adapter context, model-call resend, competing transports, and new retry/configuration machinery.

The source trace covered:

- `WyrdGatewayProvider::send` and per-attempt `AttemptRouteContext` construction;
- `PublicWyrdGatewayCaller::call`, `NativeCall::project`, and `HttpTransport::post_native`;
- the shared `AuthMiddleware::{bearer,force_refresh}` cache/exchange gates;
- `Workflows::{create,get,cancel,wait}` and drop behavior;
- `Workflow` client retention, Python load/authoring/run handoff, and mutation failure behavior;
- authenticated ingress fallback decoding and the focused Rust/Python evidence recorded by the implementer.

No build, compilation, test, package-manager, or verification command was run. Assessment is from the immutable diff, candidate source, and recorded implementation evidence only.

## Boundary assessment

| Boundary | Source and recorded evidence | Result |
|---|---|---|
| Per-call model, fallback, deadline, cancellation, and correlation | `skald-workflow/src/workflow.rs:430-518,525-548` constructs attempt-local context; `route.rs:395-427,543-590` constructs a private adapter and fresh `WyrdGatewayCall`; `wyrd-client/src/workflow/gateway.rs:75-121` projects local values and races the complete authenticated request future against cancellation and the remaining duration. The concurrent-call portion of `workflow_transport.rs:414-472` records distinct models/fallbacks on one shared caller. | PASS |
| Native 401 send-once and renewal | `wyrd-client/src/transport/http.rs:372-402` sends the model POST once, reads its original response, and invokes the established auth owner on 401 without resending. Renewal failure is propagated. `AuthMiddleware::force_refresh` serializes cache mutation through its existing gate. Recorded focused evidence covers uncoded and code-spoofing 401, successful renewal affecting only a later call, and renewal failure. | PASS — prior `FIND-TASK-003-2` is closed for this domain |
| Loaded-client context and Python mutation | `wyrd-client/src/workflow/mod.rs:33-41,123-150,159-169,244-270` retains the loading client; `wyrd-sdk-python/src/state/mod.rs:2635-2638` preserves the shared owner; `wyrd-sdk-python/src/workflow.rs:203-214,304-435,558-579` edits only the contained Skald value and runs through the retained facade. The recorded Python journey distinguishes loading server A from ambient server B and exercises successful and refused mutations. | PASS — prior `FIND-TASK-003-1` is closed for this domain |
| Remote wait and drop ownership | `wyrd-client/src/workflow/remote.rs:89-104` owns no spawned poller: one GET is awaited, then one fixed one-second sleep, so dropping the future drops only the current polling operation. `workflow_transport.rs:192-283` records terminal returns and only GET traffic for the dropped wait. | PASS |
| Ingress fallback isolation | Each public call builds a local header vector in `workflow/gateway.rs:80-97`; ingress parsing consumes the current request's authenticated `HeaderMap` and lowers only the typed override. No fallback/header/correlation state is stored on the shared caller or provider registry. | PASS |
| Active-call cancellation proof | Source composition at `workflow/gateway.rs:98-114` is direct, but `workflow_transport.rs:564-613` cancels only before calling and separately times out a delayed request. The task evidence nevertheless claims that cancellation drops caller IO. No recorded test cancels after the model POST has reached the mock boundary. | **FAIL — CONC-R2-001** |

## Proposed finding

### CONC-R2-001 — active public-gateway cancellation is not proven

- **Classification:** MISSING
- **Violated obligation:** TASK-003 Scenario 2 requires proof that cancellation drops caller IO, and REQ-043/048 plus AC-011A/013/020 require gateway-backed execution to preserve cancellation semantics at the separate caller boundary. The user's review direction makes missing or unclear required evidence an implementer finding.
- **Exact location:** `crates/shared/wyrd-client/tests/workflow_transport.rs:564-613`; the relevant implementation is `crates/shared/wyrd-client/src/workflow/gateway.rs:94-114`.
- **Evidence:** the cancellation token is already cancelled at lines 578-579 before `PublicWyrdGatewayCaller::call` is created and polled. That proves the pre-dispatch fast path only. Lines 589-612 start a delayed response but terminate it through `call.timeout`, not through the separate cancellation token. The implementation evidence describes cancellation as proven by this selector, but neither case exercises cancellation after dispatch while the request future is pending.
- **Observable consequence:** the review cannot establish from the recorded evidence that a live public-gateway request responds promptly to Workflow cancellation and drops its connection/token wait rather than remaining active until its own timeout. This is a proof gap, not a source claim that the current `tokio::select!` is incorrect.
- **Required testable correction:** extend the existing `public_gateway_call_context_and_errors` test and existing mock-server approach. Start a delayed public-gateway call, observe that its model POST reached the mock boundary, cancel that call's token, and prove the call returns the provider timeout/cancellation category promptly without a resend. Do not add a new harness, cancellation setting, retry option, checker, or transport. Record the exact focused command and result in the task evidence.

## Cancellation and sibling-call assessment

`PublicWyrdGatewayCaller::call` keeps cancellation and timeout outside `post_native`, so either winning branch drops the bearer/send/body-read/renewal future. The biased order gives cancellation precedence when cancellation and completion are simultaneously ready. The request body, fallback header, model path/body, timeout, and correlation span are call-local. Shared state is limited to the deliberately shared HTTP pool and authentication cache. The R1 change no longer turns one model call into two governed invocations; a 401-triggered refresh may affect the bearer selected by a later sibling call through the established auth owner, which is the approved shared-client behavior, not leaked Workflow call context.

The Python wrapper retains one `wyrd_client::Workflow`; authoring methods replace only its contained Skald workflow after the builder operation succeeds, leaving the retained client unchanged on both success and error. The synchronous Python run holds an immutable PyO3 borrow while the GIL is detached, so the same object cannot be mutated through a competing mutable Python borrow during that run.

## Verification evidence and limits

The implementer records passing focused Rust transport tests, the new Python registered/authored context journey, shared-family tests, Python unit/type/lint lanes, codegen, boundary checks, and formatting/lints. The source supports the R1 claims for send-once 401 behavior and retained client context. This review did not rerun any command by standing human direction.

The recorded `public_gateway_call_context_and_errors` evidence is insufficient for the active-cancellation obligation described above. A pre-cancelled token and an in-flight timeout are not evidence for cancellation of an already-dispatched request. The implementer must rerun and record the focused selector after adding that direct case.

## Standing DRIFT direction

No bespoke cancellation mechanism, refresh coordinator, setting, option, source allowlist, or review-only check is warranted. The existing `CancellationToken`, `tokio::select!`, authenticated transport, and current wiremock-based focused test are the established repository/native mechanisms. The sole remediation is direct proof through that existing surface.
