# Security Domain Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `a1792c45323489157e818eb29722913114014927`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 12, including the human-approved 2026-10-03 amendment adding `WyrdGatewayCall.model`
- Task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Review boundary: authentication and fallback-header handling, public-gateway credential containment, external-gateway secret selection and file handling, shared secret-reader relocation, error redaction, and preservation of tenant fallback policy

The candidate remained at the stated commit while this review was performed.

## Boundary, authority, and source coverage

| Boundary | Governing authority | Source and caller coverage | Assessment |
|---|---|---|---|
| Public fallback header | Spec `Implemented gateway boundary`, `WyrdGatewayCaller`, REQ-036A/038/039, INV-020, AC-011A; task Scenario 3 | `wyrd-spec/src/gateway/policy.rs`; `wyrd-server/src/components/gateway/{ingress,routes,pg_invocation_tests}.rs`; served OpenAPI test | Authentication middleware runs before handlers. Each handler checks `Caller` before decoding the header; the override is validated against the effective requested `ModelRef`; absence remains `None`; only the typed override enters `GatewayCallRequest`; provider dispatch receives no caller header. PASS. |
| Wyrd access credential and provider credential separation | Security posture trust boundaries and secret handling; spec REQ-034/038, INV-012 | `wyrd-client/src/workflow/{gateway,local}.rs`; `wyrd-client/src/transport/http.rs`; Skald `route.rs`/`plan.rs`; existing gateway credential resolver and invocation/adapter callers | The public caller sends only the Wyrd bearer to Wyrd ingress. Provider credentials remain resolved by the gateway and do not enter `WyrdGatewayCall`, Skald route state, or workflow results. One response/retry defect remains (SEC-002). |
| Selected external-gateway bindings and secrets | Spec REQ-042/058, INV-004/012; security posture secret handling and SSRF rules | `wyrd-client/src/global_config.rs`; `workflow/local.rs`; Skald `route.rs`, `plan.rs`, and endpoint policy; focused local-dependency test | `SelectedRoutes` derives only resolved routes, loads config only for selected external routes, resolves only named selected bindings at run time, and builds the complete plan before dispatch. Runtime bindings use `SecretString`, redact `Debug`, validate exact origin/protocol, reject reserved/colliding headers, and reuse the established endpoint policy. PASS. |
| Shared env/file secret reader | Security posture cryptography/secret handling; task shared-reader owner decision | `wyrd-utils/src/secret.rs`; old and new `wyrd-gateway/src/credential.rs`; `wyrd-server/src/config.rs`; their tests | Relocation preserves the established already-open-handle metadata check, regular-file and Unix group/other permission refusal, 64 KiB bound, non-UTF-8 refusal, and redacted static errors. No new external dependency entered the lockfile; `secrecy` was already workspace-resolved. PASS. |
| Native error projection | Spec `WyrdGatewayCaller`, task lines 95-115, INV-012 | `wyrd-client/src/workflow/gateway.rs`; `skald-providers::RemoteProblem`; gateway provider-refusal relay in `wyrd-gateway/src/adapter/{mod,http}.rs`; workflow transport tests | Uncoded upstream text is dropped, but an upstream can forge a catalog code and make the client retain arbitrary upstream text. FAIL (SEC-001). |
| Tenant fallback policy when override is absent | Spec REQ-036A and INV-020; task Scenario 3 | `requested_fallback`, all four public ingress call builders, gateway invocation, PG fallback test | No header produces `fallback: None`; the existing gateway then applies tenant policy. The PG test proves no per-call candidate is used without the header. PASS. |
| Dependency and supply-chain change | AGENTS dependency-cost rule; security audit manifest review | Changed manifests and `Cargo.lock` | Only existing workspace crates/dependencies moved into the runtime dependency cone (`skald-*`, `wyrd-utils`, `secrecy`). No new registry package or version was introduced. PASS. |

## Security Audit

### Critical

None.

### High

- **SEC-001 — INCORRECT** — [`crates/shared/wyrd-client/src/workflow/gateway.rs:155`](../../../../../crates/shared/wyrd-client/src/workflow/gateway.rs) treats any native error envelope containing a string found in `WyrdError::codes()` as a trusted Wyrd-generated refusal and preserves that envelope's message. The gateway deliberately relays native provider refusal JSON after removing only the resolved provider credential (`crates/wyrd/wyrd-gateway/src/adapter/mod.rs:258-308`). A compromised or tenant-defined OpenAI-compatible, Anthropic, or Gemini upstream can therefore return a known `WYRD_*` code and put a prompt, request fragment, tenant data, or other arbitrary upstream text in `message`; `Ingress::problem` copies it into `RemoteProblem`, and the workflow error projection can retain or log it. This violates the task's requirement that arbitrary upstream text, prompts, credentials, and response bodies do not survive projection and the spec's requirement that workflow-visible errors are redacted. The smallest source correction is in the existing gateway provider-refusal owner: provider-originated refusals must not be allowed to impersonate gateway-owned `WYRD_*` codes. Strip or neutralize catalog codes from relayed upstream envelopes across the three supported native protocols, so only errors generated by the authenticated Wyrd edge reach the client with a trusted Wyrd code; retain the client's existing fixed-message path for uncoded provider refusals. Add focused proof in which each native upstream returns a real catalog code plus a canary in `message`, and assert the canary is absent from `RemoteProblem` and the final workflow error while a genuine gateway-generated coded refusal retains its safe projection.

### Medium

- **SEC-002 — INCORRECT** — [`crates/shared/wyrd-client/src/transport/http.rs:350`](../../../../../crates/shared/wyrd-client/src/transport/http.rs) automatically replays every native model request after any HTTP 401. The public ingress can also relay a provider's terminal 401 after the provider has received and acted on the request (`wyrd-server/src/components/gateway/routes.rs:1339-1359`; `wyrd-gateway/src/adapter/http.rs:400-420`). Consequently, a misconfigured or malicious upstream can consume a request, answer 401, and cause `post_native` to send the complete billable model call a second time. This contradicts the method's own non-replay-safe premise and the task's required uncoded 401/403 projection to `SKALD_PROVIDERS_401_AUTH`; the current test matrix omits 401. Treat the first native 401 as the final protocol response rather than replaying a non-idempotent model call. The existing bearer acquisition already performs its normal proactive credential handling before the request; no new retry setting or transport mechanism is required. Add focused proof that one uncoded upstream 401 produces exactly one HTTP request and the required redacted auth-category `RemoteProblem`.

### Low / Defense In Depth

None. No optional hardening is required for acceptance.

### Positive Controls

- Fallback decoding is strictly after ordinary Wyrd authentication and before gateway dispatch.
- Header size, decoded JSON size, duplicate occurrence, empty candidate set, duplicate candidates, unknown fields, and self-reference are all rejected before provider IO.
- The workflow fallback header is consumed rather than forwarded, and the PG journey asserts no provider request contains it.
- `WyrdGatewayCall.model` is the sole model source used for protocol projection, closing request-body model confusion.
- Selected external bindings resolve at execution time only; unselected secret references are not read.
- Secret values use `SecretString`, runtime binding `Debug` exposes header names but not values, and reader errors omit environment names, paths, and contents.
- External gateway routes retain exact binding origin/protocol checks, reserved-header rejection, TLS/profile enforcement, and the established screened/pinned endpoint policy.
- No credential administration API, provider-body workflow context, public arbitrary-header API, or new credential-bearing configuration form was added.
- No mechanism, check, file, setting, or option lacking both repository precedent and common project practice was found; no DRIFT finding is proposed.

## Verification evidence and limits

Executed against the immutable candidate:

- `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=gateway::policy::tests::fallback_header_round_trips_and_refuses)'` — PASS (1/1).
- `mise exec -- cargo nextest run --locked -p wyrd-client --test workflow_transport -E 'test(=public_gateway_call_context_and_errors)'` — PASS (1/1).

Reviewed the task's recorded passing evidence for the authenticated PG ingress journey, shared/client/gateway/server test lanes, code generation, client-tier boundaries, PyO3 scope, and served OpenAPI. This review did not rerun the Postgres suite. The focused client test proves intended projection for coded gateway errors and uncoded 400/408/429/503 responses, but it does not exercise an upstream-forged known Wyrd code or any 401 response; those are the two closure proofs required above.

## Overall result

**FAIL**

The fallback/authentication, credential-containment, selected-secret, file-security, shared-reader, and tenant-policy boundaries otherwise satisfy the approved task, but SEC-001 and SEC-002 leave exploitable error-origin and non-idempotent replay gaps on the new public workflow gateway path.
