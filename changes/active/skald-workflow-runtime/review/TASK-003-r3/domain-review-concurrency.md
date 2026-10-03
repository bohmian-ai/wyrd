# Concurrency, cancellation, and lifecycle domain review

**Immutable subject:** base `58d07d7260df1f022a721e720a28ea48e5096e35`, candidate `0732ed92fc2907cb806ac918b09abbf6d44ff04f`.

**Overall result: FAIL.** The cumulative candidate correctly preserves request-local gateway context, loaded-client ownership, remote polling/drop behavior, send-once model dispatch, and post-dispatch cancellation. One reachable ordering defect remains in the remediated native-`401` path: renewal still waits for body collection even though the response status is already known, so caller cancellation or deadline expiry can prevent renewal from starting.

## Reviewed boundary and authority

I reviewed the complete base-to-candidate source and diff against approved Revision 12, original TASK-003, both prior verdicts and validated ledgers, and the R1/R2 remediation tasks. The human-approved `WyrdGatewayCall.model` amendment and same-spec native-`401` task-wording correction were treated as authority. Applicable obligations include TASK-003 Scenario 1 and Scenario 2, REQ-031/038/043/046/048/058, INV-020/021, AC-011A/019/020, the R2 requirement that every observed native `401` attempt renewal regardless of body collection, and the repository async/cancellation and partial-progress rules.

The source trace covered:

- `WyrdGatewayProvider::send`, `AttemptRouteContext`, and `WorkflowExecutor` cancellation/deadline precedence;
- `PublicWyrdGatewayCaller::call` through `HttpTransport::post_native` and `AuthMiddleware::{bearer,force_refresh}`;
- the post-dispatch cancellation and complete/cut-off `401` cases in `workflow_transport.rs`;
- `Workflows::{create,get,cancel,wait}`, stable create idempotency, and dropped-future ownership;
- shared `Workflow` loading/client retention and the Python wrapper's mutation/run handoff;
- selected-route dependency preparation and request-local fallback/model/deadline/correlation state.

No build, compilation, test, Cargo, mise, pnpm, pytest, formatter, linter, package-manager, or verification command was run. Conclusions use source, the immutable diff, and the implementer's recorded evidence only.

## Domain coverage

| Boundary | Source and recorded evidence | Result |
|---|---|---|
| Per-call context isolation | `crates/skald/skald-workflow/src/workflow.rs:525-548` creates attempt-local deadline, cancellation, and correlation; `route.rs:395-427,543-590` creates a private adapter and fresh `WyrdGatewayCall`; `crates/shared/wyrd-client/src/workflow/gateway.rs:77-123` keeps model, fallback headers, timeout, and trace fields local to one call. Recorded concurrent-call evidence distinguishes models and fallback headers on one shared caller. | PASS |
| Workflow cancellation and timeout precedence | `skald-workflow/src/workflow.rs:231-311,430-518` gives explicit cancellation, total deadline, step timeout, and Agent timeout biased precedence and aborts/drains Skald-owned tasks. The gateway caller separately races its HTTP future against the same cancellation token and remaining duration. | PASS |
| Post-dispatch public-call cancellation | `workflow_transport.rs:756-784` waits until the loopback boundary reports the model POST, then cancels a token whose call deadline is 300 seconds; the recorded focused selector reports the cancellation category and no resend. This closes prior `FIND-TASK-003-9`. | PASS |
| Loaded-client ownership | `wyrd-client/src/workflow/mod.rs:34-42,127-154,248-275` retains and clones the loading `WyrdClient`; `wyrd-sdk-python/src/state/mod.rs:2635-2638` and `src/workflow.rs:204-215,305-435,558-579` keep the shared owner while replacing only its Skald value. The recorded Python journey distinguishes the loading server from ambient configuration. | PASS |
| Remote lifecycle/drop ownership | `wyrd-client/src/workflow/remote.rs:41-115` uses the shared idempotent transport for create, issues one GET per wait iteration, sleeps one second only after a nonterminal snapshot, and owns no background polling or cancellation task. Rustdoc now states post-dispatch partial progress for create/cancel and drop-only polling for wait. | PASS |
| Native model send-once | `wyrd-client/src/transport/http.rs:376-410` constructs and sends one POST and never replays it after a `401`. Complete-body success/failure and cut-off-body recorded cases each observe one model POST. | PASS |
| Native `401` renewal ordering | `http.rs:402-410` records the status, then awaits the entire response body before entering `force_refresh`; `gateway.rs:100-115` may drop that whole future on cancellation or timeout. The transport has no internal total deadline (`http.rs:975-979`). | **FAIL — CONC-R3-001** |

## Proposed finding

### CONC-R3-001 — a stalled native `401` body can prevent the required renewal

- **Classification:** INCORRECT
- **Violated obligation:** The approved R1 native-`401` correction and `TASK-003-R2` require that once native response status is known to be `401`, renewal is attempted through the existing auth owner regardless of whether body collection succeeds, with no resend. The candidate rustdoc likewise says renewal follows every observed `401` so a refused credential is not reused (`crates/shared/wyrd-client/src/transport/http.rs:357-364`).
- **Exact location:** `crates/shared/wyrd-client/src/transport/http.rs:402-410`, reached through `crates/shared/wyrd-client/src/workflow/gateway.rs:96-115`.
- **Evidence and reachability:** `request.send().await` has already yielded a response and `status` is known at line 402. Line 403 nevertheless awaits `response.bytes()` before lines 404-409 begin `AuthMiddleware::force_refresh`. A server can send `401` headers and then stall or slowly stream the declared body. `post_native` applies no request timeout of its own; the public caller's `tokio::select!` drops the send future when `call.timeout` expires or the cancellation token fires. In that reachable interval renewal has not even started, and `AuthMiddleware` still holds the previously refused cached token (`auth.rs:451-460,503-529`). The next call may therefore select that token again. The R2 cut-off-body case at `workflow_transport.rs:1039-1062` closes only an immediate body-read error: its truncated connection fails promptly, allowing the later refresh branch to run. It does not cover a body that remains pending past cancellation/deadline, so the implementation record's claim that every observed `401` renews is contradicted by source.
- **Observable consequence:** a call can observe a native `401`, time out or be cancelled while reading its body, and leave the refused credential cached for a later sibling or subsequent call. This reopens the same renewal invariant as prior stable `FIND-TASK-003-2`; it does not resend the model call, but it fails the approved guarantee that renewal prepares later calls.
- **Required testable correction:** Once the response status is known to be `401`, start the existing `AuthMiddleware::force_refresh` path before body collection can indefinitely postpone it; keep renewal failure authoritative, retain the original status/body or existing body-read failure after successful renewal, and never resend the model POST. Do not add a background renewal owner, retry/configuration option, response marker, dependency, checker, or new production cancellation mechanism. Extend the existing focused deterministic HTTP proof with a `401` whose headers arrive while its body remains pending beyond a short caller deadline or cancellation: prove renewal is reached through the existing auth owner, the original call is sent once, and later authentication does not reuse the refused cached token. The implementer must rerun and record the existing focused selector.

## Lifecycle and fault-containment assessment

Dropping a public caller future cancels only local client IO. Source and rustdoc do not claim rollback after gateway acceptance, and no client-owned task survives the dropped future. Skald separately aborts and drains its own step tasks; it does not claim ownership of gateway settlement. `Workflows::create` and `cancel` accurately document that the server may have accepted their durable action before the client loses the answer, while `wait` creates no server cancellation. No shared mutable fallback, model, deadline, correlation, header map, or Workflow client-context slot was introduced.

The local raw TCP fixture added for R2 is confined to the existing focused transport test and follows loopback fixture patterns already used in the repository; it is not a production mechanism, configuration surface, or permanent check. I found no additional DRIFT mechanism, setting, option, file, or requirement in this domain.

## Verification evidence and limits

The implementer records passing focused `public_gateway_call_context_and_errors` and `shared_workflow_client_contract` selectors, shared-client tests, the Python context journey, formatting/lints, codegen, and boundary checks. Source supports the recorded post-dispatch cancellation, send-once, immediate cut-off-body renewal, retained client-context, and remote lifecycle claims.

The evidence does not exercise a native `401` whose body remains pending until caller cancellation or deadline. Because source orders body completion before renewal, that missing case is a material finding rather than a residual verification note. This review ran no command prohibited by the standing human direction. Candidate identity remained `0732ed92fc2907cb806ac918b09abbf6d44ff04f` when the report was written.
