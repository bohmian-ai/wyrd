# External-Network and Credential Security Review

## Immutable Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `28473e049705595306f2934cf4bc664168254086`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 10
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Reviewed boundary: `LlmRoute::ExtGateway`, runtime credential bindings, exact-origin enforcement, external-gateway HTTP construction, DNS screening and address pinning, redirect/proxy/TLS/IO controls, reserved headers, and credential containment through provider errors.

`HEAD` was the stated candidate before and after this review. Production source is byte-for-byte unchanged between the prior implementation commit `eb22b03f2bb766886d839bda23aafbd4ba130ab3` and this candidate; Revision 10 changes only the approved change packet.

## Authority and Source Coverage

| Boundary | Governing authority | Source and consumer coverage | Result |
|---|---|---|---|
| Runtime credential shape and remote problem shape | Revision 10 fixed seams; task packet-local public seams | `skald-workflow/src/route.rs`; `skald-providers/src/error.rs`; constructors and matches in `skald-workflow`, Agent/runtime, and gateway adapter | PASS; prior `FIND-TASK-001-3` and `FIND-TASK-001-4` are closed |
| Tenant/user-controlled endpoint egress | `architecture/agent-rules.md` SSRF rule; `architecture/wyrd-security-posture.md` Source credentials and SSRF defense; spec REQ-049, INV-010, INV-017 | `skald-providers/src/endpoint.rs`; gateway adapter, server boot/admin, and workflow consumers in the cumulative diff | PASS |
| Exact external route and binding authority | Spec REQ-038/039/042/049, INV-009/010; task ExtGateway trust contract and Scenario 5 | `wyrd-spec/src/card/workflow.rs`; `skald-workflow/src/{plan.rs,route.rs,workflow.rs}` | FAIL (`NET-R2-001`, prior `FIND-TASK-001-17`) |
| Proxy, redirect, TLS, DNS, and bounded IO | Security posture SSRF steps 1-6; spec INV-010; task Scenario 5 | `skald-providers/src/{endpoint.rs,transport.rs,clients/external.rs,clients/mod.rs}` | PASS |
| Secret containment | `AGENTS.md` secret-bearing type rules; security posture secret handling; spec REQ-042, INV-012, AC-016 | `ExternalGatewayBinding`, `merged_headers`, `ExternalGatewayClient`, `ProviderError`, Agent/Workflow projections | FAIL (`NET-R2-002`, prior `FIND-TASK-001-18`) |
| Dependency and supply-chain scope | Spec REQ-049, INV-017, AC-023 | affected manifests and `Cargo.lock` in the cumulative diff | PASS; only approved existing-workspace edges and Tokio `net` capability are used for this boundary |

## Prior-Finding Reconciliation

| Prior finding | Revision 10 and candidate evidence | Result |
|---|---|---|
| `FIND-TASK-001-3` | Revision 10 fixes `ExternalGatewayBinding.secret_headers` as `HashMap<HeaderName, SecretString>`; `route.rs:100-112` implements that exact public field. | CLOSED |
| `FIND-TASK-001-4` | Revision 10 fixes `ProviderError::RemoteProblem(Box<RemoteProblem>)`; `error.rs:59-88` implements that exact variant and payload, and direct consumers construct/match the boxed form. | CLOSED |
| `FIND-TASK-001-17` | `ExternalGatewayBindings::insert` still validates only origin and values; `merged_headers` still forwards every binding-supplied `HeaderName`. | OPEN as `NET-R2-001` |
| `FIND-TASK-001-18` | `ExternalGatewayClient` still delegates refusal handling to shared `send_json_with_retry`; `ProviderError::Status` still stores the complete bounded body and derives `Debug`. | OPEN as `NET-R2-002` |

## Security Audit

### Critical

None.

### High

- **NET-R2-001 / `FIND-TASK-001-17` — INCORRECT** — [`crates/skald/skald-workflow/src/route.rs:152`](../../../../../crates/skald/skald-workflow/src/route.rs) validates binding origins and secret values but not secret header names, while [`merged_headers` at line 466](../../../../../crates/skald/skald-workflow/src/route.rs) forwards every supplied `HeaderName`. This violates REQ-042/049, AC-016, and the task's pre-dispatch reserved-header requirement. **Exploit path:** a malicious or compromised runtime binding supplies `Host` plus its credential header; the screened TCP/TLS connection still reaches the approved origin, but a shared TLS reverse proxy can route by the attacker-selected HTTP authority to a different virtual backend, disclosing the request and binding credential outside the authorized backend. Framing, hop-by-hop, forwarding, proxy, and Wyrd-internal names similarly bypass the required outbound-header refusal. **Impact:** exact-origin authorization can be bypassed at the HTTP authority layer and credentials can reach an unintended service. **Fix:** at `ExternalGatewayBindings::insert`, reject the transport/routing reserved subset (`host`, framing/hop-by-hop, forwarding/proxy, and Wyrd-internal correlation/auth names) before storing the binding. Reuse the existing route-header classification where its categories overlap, but continue to allow ordinary binding-owned credential names such as `authorization`, `*-api-key`, and `*-token`. **Focused proof:** insertion rejects case-insensitive reserved names before dispatch, accepts ordinary credential headers, and preserves authored-header collision and origin tests.

### Medium

- **NET-R2-002 / `FIND-TASK-001-18` — INCORRECT** — [`crates/skald/skald-providers/src/clients/external.rs:135`](../../../../../crates/skald/skald-providers/src/clients/external.rs) uses the shared status path; [`clients/mod.rs:63-74`](../../../../../crates/skald/skald-providers/src/clients/mod.rs) reads and retains the complete bounded refusal body, and [`ProviderError::Status` at `error.rs:9-29`](../../../../../crates/skald/skald-providers/src/error.rs) derives `Debug` over that body. This violates REQ-042 and AC-016's prohibition on credentials in errors. A compromised external gateway already receives the bound credential and can reflect it in a 4xx/5xx body; the resulting error then contains the plaintext value and exposes it to debug formatting, crash diagnostics, or any caller inspecting the public error. The later Workflow projection uses safe display/code fields, but cannot retract the secret from the provider error that already exists. **Impact:** runtime credentials can leak through ordinary error inspection and diagnostics. **Fix:** sanitize only external-gateway non-success errors at `ExternalGatewayClient`: preserve status and retry metadata, replace the refusal body with a fixed safe diagnostic before returning the error, and leave native-provider error behavior unchanged. **Focused proof:** an external-gateway refusal reflecting a canary credential leaves the canary absent from the full returned error's `Display` and `Debug` while preserving status and retry classification.

### Low / Defense In Depth

None.

### Positive Controls

- Card route validation rejects userinfo, query, fragment, malformed or case-duplicate header names, authored credential headers, and routing/framing headers.
- Binding lookup uses a typed non-secret alias; protocol and normalized origin must match before client creation, and authored headers cannot overwrite bound secret headers.
- Secret values use `SecretString`; emitted `HeaderValue`s are sensitive; binding/client custom `Debug` implementations expose names but not values.
- The production endpoint policy rejects literal and resolved loopback/private/CGNAT/ULA addresses and always rejects metadata/link-local addresses, including IPv4-mapped IPv6 forms and mixed DNS answers containing any blocked address.
- The custom resolver hands Reqwest only the screened answer set. Redirects and environment proxies are disabled, TLS verification remains enabled, and DNS, connect, request, and response-body operations are bounded.
- `RemoteProblem(Box<RemoteProblem>)` carries only normalized safe fields and no raw response body.

## Verification Evidence

- `mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow::tests::bound_external_gateway_security)'` — PASS, 1/1.
- `mise exec -- cargo nextest run --locked -p skald-providers --lib -E 'test(=endpoint::tests::address_policy_blocks_metadata_always_and_internal_in_production) | test(=endpoint::tests::admission_follows_profile_scheme_port_and_literal_address) | test(=endpoint::tests::resolved_blocked_hosts_fail_and_redirects_are_not_followed) | test(=endpoint::tests::untrusted_certificates_are_refused_before_any_request_byte)'` — PASS, 4/4.
- Direct source comparison `eb22b03f2bb766886d839bda23aafbd4ba130ab3..28473e049705595306f2934cf4bc664168254086` over production trees — empty.

## Verification Limits

- This domain review reran the focused external-network tests, not the task's broad repository lanes.
- The focused happy/refusal test does not exercise a binding-supplied reserved header or a refusal that reflects a credential; those missing cases are the closure proofs required by the two findings.
- `git diff --check` reports an existing trailing blank line in the candidate's prior `TASK-001-r1/findings-validation.md`. That is outside this security boundary and does not change either security finding.
- Tenant-qualified server binding resolution and registered server journeys belong to later tasks; their absence is not a TASK-001 security finding.

## Overall Result

**FAIL**

Revision 10 closes the two prior public-seam blockers exactly, but it does not change production code. The independently reachable security defects preserved as `FIND-TASK-001-17` and `FIND-TASK-001-18` therefore remain and require bounded task-local remediation.
