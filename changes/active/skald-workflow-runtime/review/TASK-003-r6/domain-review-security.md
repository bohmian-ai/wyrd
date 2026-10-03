# Security domain review — TASK-003-r6

## Reviewed boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `b954976648b49429f1c0950c4fa3509573885be8`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`,
  Revision 12
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Remediation authority: TASK-003 R1 through R5 in
  `changes/active/skald-workflow-runtime/review/`

This review covers the cumulative security, authentication, authorization,
tenancy, audit, credential, network-trust, and sensitive-data effects of
TASK-003. It traces remote Workflow lifecycle requests through the shared
authenticated client; retained versus ambient client authority; public gateway
authentication and fallback-header handling; native `401` renewal and
send-once behavior; Prompt-derived model authority; selected external-secret
resolution and outbound-header containment; and portable error redaction. The
candidate remained the stated commit throughout the review.

## Authority and source coverage

| Boundary | Governing authority | Source and caller evidence | Assessment |
|---|---|---|---|
| Remote create/get/cancel/wait authority | Revision 12 REQ-031/032/032A/045/046; `architecture/wyrd-security-posture.md` identity and authorization rules | `crates/shared/wyrd-client/src/workflow/remote.rs:23-121`; `crates/shared/wyrd-client/src/transport/http.rs:701-799,801-833,865-968`; `WyrdClient::workflows` export/callers; recorded `shared_workflow_client_contract` evidence | The client sends only typed request/run identities over its existing authenticated transport. It supplies no tenant or principal selector. One request-local idempotency key survives transport retry. Authorization, tenant/owner qualification, accepted-run authority, and audit remain server-owned. PASS. |
| Retained versus ambient client authority | Revision 12 REQ-025/038/058 and R1 `FIND-TASK-003-1` | `crates/shared/wyrd-client/src/workflow/mod.rs:36-43,78-97,142-160,190-237,283-347`; Python `workflow.rs`/`state/mod.rs`; recorded Python retained-client journey | A registered or external-ref-loaded Workflow retains the complete loading `WyrdClient`, including its endpoint, credential owner, token cache, and connection pool. Local-only construction has no hidden bearer and builds an ambient client only at run start when a selected public-gateway route requires it. Python retains the same shared owner rather than copying or reconstructing authority. PASS. |
| Coherent run-start context | R3-R5 `FIND-TASK-003-10`; approved blocking boundary | `crates/shared/wyrd-client/src/workflow/mod.rs:115-160,190-237`; `workflow/local.rs:30-149`; recorded exact mixed-route and Python evidence | One `GlobalConfig::load` supplies both the selected local binding configuration and any client-less public-gateway client. Retained-client precedence is preserved, Native-only runs read no ambient configuration, and only selected binding secrets are resolved. This prevents one run from mixing credential/endpoint contexts across configuration snapshots. PASS. |
| Public gateway authentication order and identity | Security posture client-to-gateway boundary; Revision 12 REQ-038, INV-006/012/020, AC-011A | `crates/wyrd/wyrd-server/src/components/gateway/ingress.rs:42-169`; `routes.rs:1316-1346`; `components/gateway/invocation.rs:63-123,242-326,390-479,620-660`; recorded authenticated-ingress evidence | Protocol middleware extracts one permitted credential source, rejects ambiguous or URL-borne tokens, verifies the Wyrd audience, and inserts the verified principal before handlers parse fallback or dispatch. `GatewayInvocation` derives tenant and principal only from `Caller`, authorizes requested and fallback models, and uses the existing tracked canonical gateway-audit path. The fallback header conveys no tenant, principal, credential, or permission authority. PASS. |
| Fallback-header parse, bounds, and non-forwarding | Revision 12 REQ-036A, INV-020, AC-011A; synchronized `architecture/wyrd-design.md` Workflow contract | `crates/wyrd-spec/src/gateway/policy.rs:88-199`; server `ingress.rs:189-224,226-430`; `routes.rs:837-919,1316-1346,1760-1805`; recorded `public_ingress_workflow_fallback` evidence | The receiver accepts at most one 8-KiB unpadded-base64url value, caps decoded JSON at 4 KiB, deserializes a deny-unknown typed override, and rejects empty, duplicate, or requested-model candidates before dispatch. The value is consumed into `GatewayCallRequest.fallback`; provider dispatch receives neither the header nor the Wyrd bearer. Absence preserves tenant fallback policy. PASS. |
| Model-selection trust boundary | Revision 12 approved `WyrdGatewayCall.model` amendment, REQ-039/040/041 | `crates/skald/skald-workflow/src/route.rs:62-75,266-355,360-427,464-480`; `crates/shared/wyrd-client/src/workflow/gateway.rs:217-286` | The resolved Prompt provider/model produces a typed `ModelRef`. Public projection overwrites body-carried model fields or places the typed model in the path; invocation input and provider-body values cannot select another route, gateway credential, tenant, or principal. Unsupported Vertex public ingress is refused before IO. PASS. |
| Native `401` renewal and non-replay | Security posture token lifecycle; approved R1-R3 correction | `crates/shared/wyrd-client/src/auth.rs:425-590`; `transport/http.rs:351-411`; `workflow/gateway.rs:50-124`; recorded complete, renewal-failure, truncated-body, and pending-body cases in `tests/workflow_transport.rs:922-1107` | A native model POST is sent once. After response headers establish `401`, the existing authentication owner renews before untrusted body collection; renewal failure is authoritative, successful renewal affects only later calls, and the refused model request is never replayed. The caller's remaining deadline and cancellation bound local IO without claiming upstream rollback. PASS. |
| External secret and egress containment | Security posture secret and SSRF rules; Revision 12 REQ-034/042/049/058 and INV-012/017 | `crates/shared/wyrd-client/src/workflow/local.rs:72-156`; `crates/shared/wyrd-utils/src/secret.rs:1-76`; `crates/skald/skald-workflow/src/route.rs:97-199,266-355,491-550`; `skald-providers::ExternalGatewayClient` caller boundary | Secrets resolve only for binding names selected by resolved routes, at run start, and remain `SecretString`. File reads use metadata from the opened handle, require a regular owner-only Unix file, and cap reads at 64 KiB. Errors name no locator or value. Existing binding validation fixes protocol and exact origin, rejects reserved/colliding headers, uses sensitive header values, and delegates DNS/address screening, pinning, redirect, TLS, and bounded IO to the existing endpoint client. PASS. |
| Portable error, trace, and result redaction | Revision 12 native-error contract; security posture secret/audit privacy rules | `crates/shared/wyrd-client/src/workflow/gateway.rs:127-215,289-340`; `crates/skald/skald-workflow/src/attempt.rs:132-187`; tracing at `workflow/gateway.rs:68-76`; recorded OpenAI/Anthropic/Google and uncoded-category evidence | Provider text and raw bodies do not cross the portable boundary. Recognized Wyrd codes use derive-backed catalog title/remediation; uncoded errors use fixed status-category text; only the approved OpenAI `param` survives as `field`. Workflow errors retain no arbitrary details. Tracing skips bodies and credentials and records only run/step/attempt correlation. PASS. |
| Dependency and supply-chain delta | AGENTS.md dependency-cost and client-tier rules; TASK-003 non-goals | `Cargo.toml` and `Cargo.lock` cumulative diff | The manifest changes promote already-workspace Skald dependencies for production client use, reuse existing `secrecy`, and add internal `wyrd-utils` edges for the shared secret reader. No new external package or version, install hook, build script, registry, or source override is introduced. PASS. |

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None. No optional hardening is required for TASK-003 acceptance.

### Positive Controls

- Tenant and principal identity remain exclusively verified-credential derived;
  no task-added header, path, body, model, or run identifier can widen them.
- Public inference authentication finishes before fallback decoding and before
  governed dispatch, while model authorization and audit remain on the existing
  `GatewayInvocation` owner.
- Fallback overrides are request-scoped, size-bounded, duplicate-checked,
  typed, requested-model validated, consumed at ingress, and never forwarded.
- Loaded Workflows retain the client that established their registry context;
  local mixed-route runs derive both ambient authority consumers from one
  run-start configuration snapshot.
- Native model requests are send-once even across ambiguous/provider-originated
  `401` responses; renewal uses the existing auth owner before body collection
  and prepares only later calls.
- Gateway provider credentials remain server-owned. External-gateway secrets
  resolve only for selected bindings, stay in redacted types, and do not enter
  Cards, run snapshots, errors, logs, traces, observations, or audit payloads.
- Portable errors discard upstream text, response bodies, prompts,
  credentials, and arbitrary details while retaining only the approved stable
  fields.
- No nonstandard mechanism, check, file, setting, option, compatibility path,
  dependency, or security control absent from Wyrd and comparable common
  practice was added or required; no DRIFT finding is proposed.

## Prior security finding closure

| Stable finding | Security assessment |
|---|---|
| `FIND-TASK-003-1` | **CLOSED.** Rust and Python preserve the complete loading client through local execution; ambient configuration cannot replace that authority. |
| `FIND-TASK-003-2` | **CLOSED.** Every observed native `401` starts renewal before body collection, renewal failure wins, and exactly one model POST is sent with no replay. |
| `FIND-TASK-003-3` | **CLOSED.** Provider-controlled message text is discarded even beside a recognized Wyrd code; catalog metadata owns the portable message and remediation. |
| `FIND-TASK-003-4` | **CLOSED.** The plaintext file reader is private; cross-module secret consumers receive `SecretString`. |
| `FIND-TASK-003-5` through `FIND-TASK-003-9` | **CLOSED / no security regression.** The source-contract, cancellation-proof, and import/documentation remediations preserve security behavior. |
| `FIND-TASK-003-10` | **CLOSED.** One run-start configuration snapshot supplies both mixed-route authority consumers, on the approved blocking boundary, with selected-only secret resolution. |
| `FIND-TASK-003-11` | **CLOSED.** The auth owner distinguishes replay-safe transport retry from native send-once renewal. |
| `FIND-TASK-003-12` | **CLOSED.** Consuming the shared facade documents loss of its retained client and automatic dependency composition. |
| `FIND-TASK-003-13` | **CLOSED.** Active architecture now records authentication order, encoding, bounds, refusal, non-forwarding, and absent-header behavior. |
| `FIND-TASK-003-14` and `FIND-TASK-003-15` | **CLOSED / no security regression.** Rust and Python public contracts now accurately disclose ambient authenticated IO and selected secret reads without changing runtime authority. |

## Evidence limits

This was a strictly read-only, source-only review. I did not build, compile,
run tests, invoke Cargo, mise, pnpm, pytest, formatters, linters, type checkers,
code generation, or any verification lane. The repository has no `.codegraph/`
index, so navigation used the immutable base-to-candidate diff and ordinary
source search. I inspected the approved specification, original task, R1-R5
remediation packets and their recorded evidence, applicable architecture and
security authority, cumulative manifests, changed source, expanded callers,
and the task-owned test source.

Recorded evidence is specific and consistent with source for authenticated
fallback handling and non-forwarding, malformed/repeated/oversized/self-listed
fallback refusal, per-call isolation, retained-client context, selected-secret
resolution, remote lifecycle projection, native send-once complete/truncated/
pending-body `401` renewal, and portable-error redaction. No required security
evidence is missing, unclear, or contradicted by the candidate.

## Material findings

None.

## Overall result

**PASS**

The cumulative candidate satisfies TASK-003's approved security,
authentication, authorization, tenant-attribution, audit-boundary,
credential-containment, fallback-validation, native-renewal, and redaction
obligations. No material security finding is proposed.
