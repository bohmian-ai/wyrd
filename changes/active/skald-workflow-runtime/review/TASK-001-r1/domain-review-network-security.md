# Domain Review: External-Network Security

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `eb22b03f2bb766886d839bda23aafbd4ba130ab3`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, revision 9
- Task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Reviewed boundary: `LlmRoute::ExtGateway`, runtime credential bindings, external-gateway HTTP construction, DNS screening/address pinning, redirects, proxy/TLS/IO bounds, and credential redaction from transport through Workflow results and observers.

The candidate remained at the stated commit during this review.

## Authority and Source Coverage

| Boundary | Governing authority | Source and consumer coverage | Result |
|---|---|---|---|
| Tenant/user-controlled endpoint egress | `AGENTS.md` §§4, 6, 9; `architecture/agent-rules.md` SSRF rules; `architecture/wyrd-security-posture.md` "Source credentials and SSRF defense"; spec REQ-049, INV-010, INV-017 | `skald-providers/src/endpoint.rs`; moved `wyrd-gateway` consumers in `adapter/http.rs`; boot/test construction references | PASS |
| Exact external route and binding authority | Spec route contract, REQ-042, REQ-049, INV-009–010; task packet-local ExtGateway trust contract and Scenario 5 | `wyrd-spec/src/card/workflow.rs`; `skald-workflow/src/{plan.rs,route.rs,workflow.rs}` | FAIL (`NET-001`) |
| DNS answer validation and connection pinning | `architecture/agent-rules.md`; security posture steps 2–5; task Scenario 5 | `EndpointPolicy::permits`, `ScreeningResolver::resolve`, `EndpointPolicy::screened` and tests | PASS |
| Proxy, redirect, TLS, and bounded IO | Security posture steps 1, 4–6; spec INV-010; task Scenario 5 | `EndpointPolicy::transport`; `HttpTransport`; `ExternalGatewayClient`; endpoint TLS/redirect tests | PASS |
| Secret containment | `AGENTS.md` §4; security posture security principles and secret handling; spec REQ-042, INV-012, AC-016 | `ExternalGatewayBinding`, `merged_headers`, `ExternalGatewayClient`, `ProviderError`, Agent journal/observer projections, `project_agent_error` | FAIL (`NET-002`) |
| Dependency/supply-chain scope | Spec REQ-049; task material stop conditions | crate manifests and `Cargo.lock` | PASS: the only production capability change is Tokio's already-approved `net` feature plus existing workspace edges; `rcgen` and `rustls` are test-only existing workspace dependencies. |

## Security Audit

### Critical

None.

### High

- **NET-001 — INCORRECT** — [`crates/skald/skald-workflow/src/route.rs:152`](../../../../../crates/skald/skald-workflow/src/route.rs) accepts every syntactically valid `HeaderName` in `ExternalGatewayBinding.secret_headers`, and [`merged_headers` at line 466](../../../../../crates/skald/skald-workflow/src/route.rs) sends it unchanged. The binding validation checks only secret values; unlike Card-authored headers, it never rejects `host`, framing/hop-by-hop headers, forwarding headers, proxy headers, `x-wyrd-access-token`, or `wyrd-request-id`. A configured or compromised binding can therefore set `Host` while the screened connection and TLS SNI still target the bound origin. On a shared TLS reverse proxy, `Host` can select a different virtual backend, so the request and the binding's other credentials can reach an authority other than the exact origin the binding approved; framing/proxy headers also bypass the packet's required reserved-outbound-header refusal. This violates the exact-origin/header constraint in REQ-042/049, the pre-dispatch forbidden-header check at spec lines 1311–1312, and Scenario 5. **Required correction:** make `ExternalGatewayBindings::insert` reject the transport/routing reserved-header subset before storing the binding, while continuing to permit credential-bearing names such as `authorization`, `*-api-key`, and `*-token` that are forbidden only to Card authors. Reuse the route header classification where its categories overlap instead of adding a second unrelated validation path. **Closure proof:** a focused binding-insertion test must reject case-insensitive `Host`, hop-by-hop/framing, forwarding/proxy, and Wyrd-internal correlation/auth headers before dispatch, while accepting an ordinary bound `Authorization` or API-key header; the existing authored-header and collision cases must remain green.

### Medium

- **NET-002 — INCORRECT** — [`crates/skald/skald-providers/src/clients/external.rs:135`](../../../../../crates/skald/skald-providers/src/clients/external.rs) delegates external-gateway failures to the shared `send_json_with_retry`; [`clients/mod.rs:63`](../../../../../crates/skald/skald-providers/src/clients/mod.rs) reads the complete bounded refusal body and [`ProviderError::Status` at `error.rs:21`](../../../../../crates/skald/skald-providers/src/error.rs) retains that body in an error whose derived `Debug` exposes it. A compromised external gateway already receives the bound credential and can reflect it in a 4xx/5xx body; the resulting external-gateway error then contains the plaintext credential and can disclose it through ordinary debug logging or inspection before the later Workflow projection discards provider bodies. Marking outbound header values sensitive and using a redacted `Display` does not redact the stored body or derived `Debug`. This violates REQ-042's prohibition on secret values in errors/logs and AC-016's credential-disclosure condition. **Required correction:** at the external-gateway client boundary, retain only status and retry metadata for non-success answers and replace the upstream body with a fixed safe diagnostic before the error can enter Agent/Workflow state; do not change native-provider error behavior, which REQ-049 excludes. **Closure proof:** a focused external-gateway test must return an error response that reflects a canary secret and prove the canary is absent from the complete error value's `Display` and `Debug`, the projected Workflow result, journal/observer messages, and retry classification.

### Low / Defense In Depth

None.

### Positive Controls

- `LlmRoute::validate` rejects userinfo, query, fragment, invalid or duplicate authored header names, credential-like Card headers, and routing/framing headers.
- Binding lookup is by typed non-secret alias; protocol and normalized URL origin must match before client construction, and authored headers cannot overwrite bound secret headers case-insensitively.
- Secret header values use `SecretString`, are converted to sensitive `HeaderValue`s, and are omitted from binding/client `Debug` output.
- Production endpoint policy rejects literal and resolved loopback/private/CGNAT/ULA addresses, always rejects metadata/link-local addresses, normalizes IPv4-mapped IPv6 addresses, and rejects the entire DNS answer if any address is blocked.
- The custom Reqwest resolver returns only the screened socket addresses used by the connection, closing the DNS check/use gap. Redirects and proxies are disabled, certificate verification remains enabled, and response bodies, DNS, connect, and request operations are bounded.

## Verification Evidence

- `mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow::tests::bound_external_gateway_security)'` — PASS, 1/1.
- `mise exec -- cargo nextest run --locked -p skald-providers --lib -E 'test(=endpoint::tests::address_policy_blocks_metadata_always_and_internal_in_production) | test(=endpoint::tests::admission_follows_profile_scheme_port_and_literal_address) | test(=endpoint::tests::resolved_blocked_hosts_fail_and_redirects_are_not_followed) | test(=endpoint::tests::untrusted_certificates_are_refused_before_any_request_byte)'` — PASS, 4/4.
- `git diff --check <base>..<candidate>` — PASS.

## Verification Limits

- This domain pass did not rerun the task's broad repository lanes; it used the recorded green evidence as available evidence and reran the focused external-network tests above.
- TASK-001 intentionally supplies the runtime transport and binding primitives only. Tenant-qualified server binding resolution and real local/server registered-workflow journeys are assigned to later tasks, so their absence is not a TASK-001 finding.
- The resolver tests exercise blocked hostname resolution and the implementation rejects a mixed answer through an `all` check, but there is no deterministic injected-resolver test for a public-plus-blocked answer or DNS answer change. The source path is direct and no resolver trait may be added under this task; this is a proof limit, not a separate finding.

## Overall Result

**FAIL**

The candidate has two bounded, task-local security defects: runtime bindings can override the HTTP authority/reserved transport headers, and external-gateway refusal bodies can retain reflected credentials in an error value. Both corrections belong at existing shared owners and require no product or architecture revision.
