# TASK-003-r2 system-resilience review

## Review Findings

### Critical

None.

### Important

None.

### Suggestions

None. No resilience mechanism, check, file, setting, or option was found that
needs to be added beyond the established client, authentication, gateway, and
runtime owners.

## Immutable Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `2e6f13f505daf19050e588ea0d1d7fc936697e27`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`,
  Revision 12, including the human-approved 2026-10-03 minimal amendment adding
  `pub model: ModelRef` to `WyrdGatewayCall`
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Prior verdict and ledger:
  `changes/active/skald-workflow-runtime/review/TASK-003-r1/{verdict.md,findings-validation.md}`
- Remediation task:
  `changes/active/skald-workflow-runtime/review/TASK-003-r1/TASK-003-R1-preserve-client-context-and-native-call-safety.md`,
  including the human-approved same-spec native-401 wording correction

The candidate remained `HEAD` throughout this review.

## Deployed-Path Evidence

| Runtime path | Source evidence | System effect |
|---|---|---|
| Shared local Workflow ownership | `crates/shared/wyrd-client/src/workflow/mod.rs:33-40,74-93,123-149,244-270` | A registered Workflow keeps the `WyrdClient` owned by its Cards handle; an authored Workflow with external refs keeps the ambient client that resolved them. At execution, only a selected `wyrd_gateway` route clones that retained client. A purely local Workflow constructs no client during loading. |
| Python runtime bridge and context handoff | `sdks/wyrd-sdk-python/src/state/mod.rs:2635-2638`; `sdks/wyrd-sdk-python/src/workflow.rs:558-579` | Python now retains the complete shared `wyrd_client::Workflow` across load, authoring edits, and `wyrd_runtime` execution. It no longer reconstructs a context-free owner at run time, so changing or removing ambient configuration cannot redirect an already loaded Workflow. The bridge uses the repository runtime and releases the GIL while it blocks; no second runtime or background owner is introduced. |
| Selected local dependencies and secrets | `crates/shared/wyrd-client/src/workflow/local.rs:38-102,105-135` | Route discovery is synchronous and per Workflow. Only selected external bindings resolve their secret refs, on Tokio's blocking pool, before dependencies are installed. A missing/invalid selected binding fails before dispatch, while unselected bindings and unrelated capabilities remain untouched. |
| Public Wyrd gateway call | `crates/shared/wyrd-client/src/workflow/gateway.rs:50-122`; `crates/shared/wyrd-client/src/transport/http.rs:350-401` | Model, fallback, remaining duration, cancellation, and correlation are call-local. One native model POST is sent through the existing authenticated transport. The outer `select!` bounds bearer acquisition, send, body read, and any reactive refresh by the step's remaining duration and cancellation token. |
| Native `401` recovery | `crates/shared/wyrd-client/src/transport/http.rs:373-401`; `crates/shared/wyrd-client/src/auth.rs:503-541` | A `401` cannot prove whether refusal occurred at Wyrd authentication or after governed provider dispatch. The candidate therefore never resends the non-idempotent model POST. It invokes the existing authentication owner once so a later call can use renewed credentials; refresh failure returns the owner's authentication error, while successful refresh preserves the original status/body. Fixed bearer credentials remain non-renewable without creating another credential mechanism. |
| Public ingress and governed dispatch | `crates/wyrd/wyrd-server/src/components/gateway/ingress.rs:101-139,198-224`; `crates/wyrd/wyrd-server/src/components/gateway/routes.rs:1332-1380,1769-1805` | Authentication precedes handler interpretation. The fallback header is decoded and validated against the requested model before `GatewayInvocation`; duplicate, malformed, oversized, empty, or self-referential overrides dispatch nothing. Only the typed override enters the established gateway owner, and the header is not forwarded upstream. |
| Native refusal normalization | `crates/shared/wyrd-client/src/workflow/gateway.rs:155-212` | Provider-controlled envelope text is discarded. A recognized Wyrd code uses derive-backed catalog title/remediation; uncoded or unknown refusals use the existing provider status category and fixed message. The failure remains a bounded `RemoteProblem` rather than leaking an upstream body into Workflow state. |
| Remote Workflow lifecycle handle | `crates/shared/wyrd-client/src/workflow/remote.rs:23-104` | Create uses the shared idempotent submission path; get and cancel are ordinary authenticated requests. Wait performs one GET per second, returns every terminal state as a snapshot, and dropping the future stops polling only. The accepted run remains server-owned and is neither resubmitted nor cancelled by a dropped waiter. |

## Failure, Recovery, and Sibling-Capability Assessment

| Failure or recovery path | Observed behavior | Assessment |
|---|---|---|
| Wyrd server or network unavailable before/during a local public-gateway call | The authenticated exchange fails within the per-call timeout/cancellation boundary and maps to the existing provider connection/timeout category. Workflow policy remains the owner of any later step attempt. There is no transport-level model replay or unbounded retry. | PASS |
| Upstream provider returns `401` after governed dispatch | The response body is read once, the current Wyrd credential is offered to `AuthMiddleware::force_refresh`, and the original model POST is never resent. A successful refresh affects only later calls; a refresh failure returns the established authentication error. | PASS. This closes prior `SYS-001` / `FIND-TASK-003-2` without provenance markers, retry settings, or another transport. |
| Provider returns a Wyrd-looking code with sensitive or misleading text | The client trusts only a code present in the local catalog, derives its portable message/remediation from that catalog, and discards envelope text for OpenAI, Anthropic, and Google forms. Unknown/uncoded responses retain only the fixed category fields. | PASS. The failure cannot expand retained Workflow diagnostics or affect ordinary native relay behavior. |
| Cancellation or step deadline while sending, reading, or refreshing | `PublicWyrdGatewayCaller::call` races the complete future against the cancellation token and remaining timeout. Dropping client IO cannot mutate the completed/terminal Workflow snapshot; any already accepted server-side gateway operation remains bounded and settled by the existing gateway owner. | PASS for the approved local-call boundary. |
| Python ambient configuration changes after registered or authored-ref loading | `PyWorkflow` retains the shared owner and calls `self.inner.run`, so the original server/credential remains authoritative through later authoring edits. A locally built Workflow still resolves ambient configuration lazily when its selected route first needs a Wyrd client. | PASS. The remediation restores Rust/TypeScript-equivalent ownership instead of copying tokens or configuration into Python. |
| Selected secret source is unavailable, slow, invalid, or unsafe | Resolution occurs only for a selected external binding on the blocking pool and fails before provider dispatch. The shared reader retains its existing regular-file, size, permission, and redaction behavior; making its plaintext helper private does not change runtime recovery. | PASS. Failure is run-scoped and does not stop unrelated client or server capabilities. |
| Concurrent local gateway calls | Each call builds its own body, fallback header collection, timeout, token wait, and correlation span. Shared state is limited to the existing connection pool and auth cache. No fallback, deadline, or correlation value is written into shared adapter/provider state. | PASS. A dependency refusal may cause one bounded refresh request per refused call, but no model call is replayed and the task does not introduce an unbounded retry or queue. |
| Remote polling is interrupted or the client process exits | The polling future and its one-second timer disappear. No cancellation or second create is sent, and the accepted server job continues under its own lifecycle. A later caller may resume with `get`/`wait` while the owning server process remains available. | PASS. |
| Owning server process or replica is lost | This task adds the client projection only. Revision 12 intentionally defines Workflow runs as process-local and deployment-affine; restart loses run and deduplication state, and request affinity is an operator requirement. The candidate does not claim recovery, persistence, or cross-replica lookup. | PASS for TASK-003; adding a lease, durable queue, or recovery setting would be DRIFT. |
| Malformed or hostile fallback header | Authentication still occurs first. Header validation fails before `GatewayInvocation`, so no provider call, fallback attempt, accounting, or capture work starts. Other gateway operations and clients omitting the header retain their existing behavior. | PASS. |
| Auth/token endpoint unavailable during reactive renewal | The original model call is not repeated. The call fails through the existing auth catalog error, remains bounded by the Workflow call deadline/cancellation, and unrelated server gateway clients are unaffected. | PASS. |

## Affected Capabilities and Recovery Boundary

The candidate affects local Workflow execution in Rust, Python, TypeScript, and
their shared client process; shared remote create/get/cancel/wait calls; and the
existing public OpenAI, Anthropic, and Gemini gateway ingresses. It does not add
a server-run owner, durable run state, gateway credential owner, provider
transport, health loop, retry worker, or recovery protocol. Native provider and
external-gateway routes keep their existing owners. The remediation changes
only Python retention of the shared Workflow owner, native call resend/error
projection, and private/static code shape; it does not widen the deployed
failure domain.

Failure remains request- or run-scoped. A local client failure can terminate
that Workflow step/run but cannot crash `wyrd-server` or disable unrelated
Cards, Bifrost, gateway, or SDK operations. Conversely, a server-accepted
gateway call may finish its existing bounded settlement after client
cancellation, but has no route back to mutate a terminal local WorkflowRun.

## Open Questions

None.

## Verification Notes

- Per the human standing direction, this review ran no build, compile, test,
  lane, Cargo, mise, pnpm, pytest, package-manager, or other verification
  command. Assessment is from the cumulative source/diff and the implementer's
  recorded evidence only.
- The remediation records the focused
  `public_gateway_call_context_and_errors` result as passing after proving:
  uncoded and Wyrd-code-spoofing `401` responses each send one model POST;
  successful renewal affects the next bearer without replay; failed renewal
  returns the auth error; and provider-controlled known-code messages do not
  cross into `RemoteProblem`.
- The remediation records the Python integration journey
  `test_loaded_workflow_calls_the_gateway_through_its_loading_client` as passing
  for both registered and authored-external-ref loads, conflicting and absent
  ambient configuration, successful and refused authoring edits, and a wholly
  local Workflow's pre-dispatch missing-client refusal.
- The source at `crates/shared/wyrd-client/tests/workflow_transport.rs:731-865`
  and
  `sdks/wyrd-sdk-python/tests/integration/gateway/test_workflow_gateway_context.py:123-181`
  matches those recorded claims. No missing, unclear, or source-contradicted
  resilience proof was identified.
- Broader recorded shared, Python, gateway, SDK, OpenAPI, codegen, boundary,
  formatting, and lint results were inspected as evidence but were not rerun.
  TASK-004/005 remain the approved owners of real accepted-run and complete
  cross-language Workflow journeys; their absence is not a TASK-003 resilience
  gap.

## Overall Result

**PASS**

The cumulative candidate satisfies TASK-003's deployed and recovery boundaries.
The prior duplicate-dispatch risk and Python client-context loss are corrected
at their existing owners, the recorded focused evidence agrees with source,
and no bounded system-resilience finding remains.
