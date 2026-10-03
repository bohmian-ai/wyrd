# TASK-003 focused follow-up review

## Subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `a1792c45323489157e818eb29722913114014927`
- Task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Authority: approved Revision 12, including the human-approved 2026-10-03 `WyrdGatewayCall.model` amendment.
- Scope: conflicting native HTTP 401 replay and recognized-code error-message corrections.

**RESOLVED.** The fresh follow-up reviewer returned this report for the orchestrator to preserve because its reviewer role prohibits file edits. The candidate remained unchanged. No builds, tests, lanes, package managers, or verification commands were run. Historical reviewer reruns are not acceptance evidence; the implementer's recorded task evidence was assessed alongside source.

## Authority and source coverage

The reviewer read the task, approved specification, repository rules, spec-driven development and maintainer guidance. `architecture/wyrd-design.md:553-556` requires SDK token re-exchange when refused; TASK-003:77-83 preserves request-local auth refresh/retry. TASK-003:95-115 requires safe common error projection, stable codes when present, and uncoded 401/403 category mapping. Architecture re-exchange and task-directed same-call replay are distinct obligations.

| Owner/path | Evidence |
|---|---|
| `AuthMiddleware::bearer`, `auth.rs:451-499` | Cached renewable credentials exchange when stale/missing; a supplied bearer is returned directly. |
| `AuthMiddleware::force_refresh`, `auth.rs:512-541` | Existing owner re-exchanges renewable credential forms; supplied bearer stays unchanged. Renewal does not establish replay safety. |
| `HttpTransport::post_native`, `transport/http.rs:365-401` | Its sole production caller is `PublicWyrdGatewayCaller`; first 401 refreshes and repeats the POST before body inspection. |
| Shared HTTP siblings, `transport/http.rs:210-244,850-952` | Existing framed/JSON operations have their own auth/retry paths and need not change. |
| `GatewayIngress::authenticate/error`, `ingress.rs:81-139`; `token_extract.rs:146-164`; `http/error.rs:212-260` | Genuine pre-dispatch authentication errors use ordinary native envelopes and catalog codes, including token-expired/invalid-token. |
| `adapter::failure`, `wyrd-gateway/src/adapter/http.rs:400-422` | A provider status refusal is an already-dispatched attempt and keeps its status. |
| `adapter::refusal`, `adapter/mod.rs:258-308` | Native refusal JSON survives credential scrubbing; no reserved-code removal or trusted-origin discriminator exists. Translated refusal has a separate envelope path. |
| `GatewayInvocation`, `invocation.rs:553-559`; public relay, `routes.rs:1332-1360` | Completed provider refusal is relayed as its status/body without an origin discriminator. |
| `Ingress::problem`, `workflow/gateway.rs:155-209`; consumer, `skald-workflow/src/attempt.rs:170-184` | Recognized code admits body message into `RemoteProblem`, then WorkflowRunError; no later origin check repairs it. |

Ordinary native gateway callers share the provider relay. Stripping provider codes there changes sibling public behavior, rather than merely repairing the new Workflow client.

## Native 401 replay conflict — FU-001

Classification: **INCORRECT**. Location: `crates/shared/wyrd-client/src/transport/http.rs:394-398`.

Wyrd can authenticate a caller, dispatch the model request, receive a provider 401, and relay it. `post_native` interprets that status as edge refusal and sends the complete model POST again. This duplicates provider work and gateway accounting/capture/audit for one call. A fixed bearer is replayed unchanged too.

Code membership does not resolve origin: a provider controls the native envelope and can supply `WYRD_AUTH_401_TOKEN_EXPIRED`. Both status-only and known-auth-code gating remain unsafe. No existing provenance or WWW-Authenticate discriminator identifies edge refusals.

| Proposed correction | Source assessment |
|---|---|
| Gate replay on recognized Wyrd authentication codes | Still spoofable by a native provider. |
| Strip recognized codes at provider relay | Changes established ordinary native relay and sibling consumers. |
| Delete replay and rely only on proactive `bearer()` | Prevents duplication but does not preserve reactive re-exchange on refusal. |
| Preserve `AuthMiddleware` renewal, return original refusal, never resend this POST | Smallest existing-owner option preserving renewal and non-idempotent safety; correct the private task retry instruction through `$wyrd-plan` if it requires same-call replay. |
| Require successful same-call replay while preserving native relay | Existing response information is insufficient; this requires a response-provenance authority decision. |

The resend decision belongs to `HttpTransport::post_native`; credential renewal remains on `AuthMiddleware`. If approved authority requires same-call replay, route that public provenance decision to specification authority instead of inventing a marker or changing relay silently. The independent validator must resolve the authority route.

Implementer closure proof: uncoded provider 401 and provider 401 spoofing a real auth code each cause one model request; uncoded refusal projects to `SKALD_PROVIDERS_401_AUTH`; credential renewal follows the approved/task-corrected outcome. If same-call replay is required, prove genuine pre-dispatch origin and rejection of an upstream lookalike. A mocked coded body alone cannot prove origin. `workflow_transport.rs:664-691` omits 401; recorded green evidence does not close this path.

## Recognized-code message conflict — FU-002

Classification: **INCORRECT**. Location: `crates/shared/wyrd-client/src/workflow/gateway.rs:190-198`.

A provider can return a known Wyrd code with arbitrary prompt/diagnostic text in an OpenAI message, Anthropic string code/message, or Google ErrorInfo reason/message. The native relay preserves it; the new client recognizes the code and copies the message into the portable error. Catalog membership validates spelling/metadata, not origin. The unknown-code fixed-message branch does not protect this path.

The smallest correction belongs to the public client's normalization boundary: retain required status, recognized code and catalog remediation, but derive the portable message from trusted existing catalog metadata (such as title). Leave the ordinary public provider relay and uncoded-category behavior intact. This consumer legitimately owns the guard because it promises RemoteProblem's safety invariant. A producer rewrite is unnecessary and affects siblings; no second catalog, provenance header, setting, framework or dependency is needed.

Implementer closure proof: extend the existing focused client test with OpenAI, Anthropic and Google recognized-code canary messages; verify no canary enters RemoteProblem or WorkflowRunError while required status/code/remediation survive. Check genuine gateway-coded examples under the selected safe-message projection. Current tests at `workflow_transport.rs:572-662` preserve coded messages and the canary cases at 664-691 cover unknown codes only.

## Shared cause and correction boundary

Both failures mistake native response representation for origin, but neither local correction fixes both outcomes. Catalog-derived text closes message leakage without establishing pre-dispatch origin. Removing resend closes duplicate work without sanitizing messages. Keep the two consumer outcomes explicit; consolidating them into trusting recognized codes preserves the vulnerability.

## Evidence and standing direction

Source navigation and Git inspection only. The task records passing transport/gateway/shared/SDK/ingress/OpenAPI/boundary checks, but omits the two failing states. Missing closure proof belongs to the implementer. No novel mechanism is required by the bounded redaction correction. Replay authority is explicitly routed for independent validation; the report does not select an overall task verdict.
