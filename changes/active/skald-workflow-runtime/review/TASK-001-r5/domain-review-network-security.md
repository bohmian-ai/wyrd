# Network-security domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `09e4b82c2a4cab3ea27e0889d3acf3ea22c2b596`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 11 (`REQ-053` included)
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Final remediation authority: `changes/active/skald-workflow-runtime/review/TASK-001-r4/TASK-001-R3-close-round-four-review-gaps.md`

The candidate remained at the stated commit throughout this audit. No
`.codegraph/` index exists, so review used the complete immutable diff,
repository search, and direct source/caller tracing. The round-four reports and
findings were used only as closure hypotheses.

## Reviewed boundary, authority, and source coverage

| Boundary | Authority and source coverage | Result |
|---|---|---|
| Declarative external route | Revision 11 REQ-036, REQ-039, REQ-042, INV-009/010/012; `crates/wyrd-spec/src/card/workflow.rs:497-669`; resolved-plan validation in `crates/skald/skald-workflow/src/plan.rs` | PASS: routes reject userinfo, query, fragment, malformed/duplicate headers, authored credential headers, fallback misuse, and request-dialect mismatch before dispatch. |
| Runtime credential binding | REQ-034/042/049, INV-010/012/017; `crates/skald/skald-workflow/src/route.rs:95-198,264-345,463-503` | PASS: bindings are runtime-only, redact values in `Debug`, require a bare origin, validate reserved names and values, match exact protocol and normalized origin, and refuse Card-authored collisions before constructing a client. |
| SSRF resolution, screening, and pinning | Repository SSRF rules in `architecture/agent-rules.md`; REQ-049, INV-010/017; `crates/skald/skald-providers/src/endpoint.rs:23-216`; `crates/skald/skald-providers/src/clients/external.rs:68-144` | PASS: literal addresses and every resolved address are screened; a mixed allowed/blocked answer rejects the whole lookup; the screened resolver supplies the connection addresses, so there is no unchecked re-resolution hop. Metadata/link-local addresses are blocked in every profile and production additionally blocks loopback/private/CGNAT/ULA/unspecified addresses. |
| Transport boundary | INV-010 and AC-016/023; `crates/skald/skald-providers/src/endpoint.rs:98-130`; gateway consumer `crates/wyrd/wyrd-gateway/src/adapter/http.rs:51-120`; production composition `crates/wyrd/wyrd-server/src/boot/mod.rs:1464-1477` | PASS: certificate verification remains enabled, proxies and redirects are disabled, and DNS/connect/request/body work is bounded. Production boot selects the production policy. The old gateway-local implementation was deleted rather than duplicated. |
| Credential containment | REQ-042, INV-012, AC-016; `crates/skald/skald-providers/src/clients/external.rs:146-235`; `crates/skald/skald-workflow/src/workflow.rs:2091-2632` | PASS: final refusal bodies and decode diagnostics are withheld; bound values are sensitive header values; decoded successful responses are recursively checked after JSON unescaping and refused with a fixed terminal error before Agent or Workflow projection when retained strings or member names reflect a credential. |
| Remote problem projection and retry | Revision 11 remote-problem and retry contracts; `crates/skald/skald-providers/src/error.rs:59-88,154-175`; `crates/skald/skald-workflow/src/attempt.rs:17-23,128-193`; `crates/skald/skald-workflow/src/route.rs:505-567` | PASS: projection retains only stable code, safe message, status, optional field, and catalog remediation; Workflow results retain only `details.field`. Only the three approved gateway codes retry, while auth/authorization/binding/route/reflection refusals remain terminal. |
| Request and step isolation | REQ-038/043, INV-020; `crates/skald/skald-workflow/src/route.rs:367-408,505-606`; `crates/skald/skald-workflow/src/workflow.rs:1838-2089` | PASS: each step attempt gets a private adapter carrying its own fallback, deadline, cancellation, correlation, model, and client; there is no mutable global route/request state to exchange metadata or credentials between concurrent steps. |
| Tenant isolation scope | REQ-034 and AC-023; TASK-001 owners and consumers | PASS for TASK-001: this task supplies the runtime binding collection but does not implement server configuration or tenant-qualified lookup. No candidate path accepts tenant identity from invocation data or adds cross-tenant lookup/listing. The server-qualified binding owner remains an explicitly deferred TASK-004 boundary, not a missing TASK-001 control. |
| Final remediation and reported teardown residual | `a704a8890..09e4b82c2`; `crates/wyrd/wyrd-testing/src/server.rs`; remediation findings FIND-TASK-001-27/-28/-29 | PASS for this domain: the final remediation changes Skald documentation, Agent model-span attribution, and test-only server lifecycle settlement. It does not change authentication, authorization, endpoint policy, credential ownership, header construction, remote-problem projection, retries, or tenant/request isolation. In-process test teardown taking Bifrost's abort fallback is a harness lifecycle/resilience question; it creates no reachable network-security or credential-disclosure regression. |

## Prior-finding closure

- `FIND-TASK-001-27`: no network-security effect. The documentation correction
  preserves the narrow dependency boundary and introduces no application-tier
  dependency into Skald.
- `FIND-TASK-001-28`: no network-security regression. The test-server
  lifecycle change does not weaken endpoint, authentication, authorization, or
  secret controls. The reported in-process Bifrost abort path is outside this
  domain and does not reopen a security finding.
- `FIND-TASK-001-29`: no network-security regression. Recording the effective
  post-callback model changes payload-free telemetry attribution only and does
  not expose the request or credentials.

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None. No optional hardening outside the approved task is promoted to a
finding.

### Positive Controls

- Runtime credentials use `SecretString`; binding and dependency diagnostics
  expose names and capability presence, never values.
- Secret headers are marked sensitive before transport, and the same marking
  drives successful-response reflection refusal.
- Protocol, exact origin, scheme/profile, header, and credential-collision
  checks complete before dispatch.
- DNS screening rejects the entire answer on any blocked address and supplies
  the exact address set used by the connection.
- Redirects, environment proxies, invalid TLS certificates, oversized bodies,
  and bounded timeouts fail closed.
- External refusal bodies, decode diagnostics, and reflected successful
  responses are contained at the external-client owner before public error or
  result projection.
- Remote Wyrd errors retain only the approved safe common fields, and
  security/binding failures cannot become Workflow retries.

## Verification evidence and limits

Executed against candidate `09e4b82c2a4cab3ea27e0889d3acf3ea22c2b596`:

- `mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow::tests::external_gateway_success_reflection) | test(=workflow::tests::bound_external_gateway_security) | test(=workflow::tests::isolated_route_calls)'` — **3 passed**.
- `mise exec -- cargo nextest run --locked -p skald-providers --lib -E 'test(endpoint::tests::)'` — **4 passed**.
- `git diff --check a51af030b6039eea4b2914f3ebf2c31925d08721..09e4b82c2a4cab3ea27e0889d3acf3ea22c2b596` — **passed**.

The focused proofs cover TASK-001's local ExtGateway, endpoint-policy,
credential-containment, remote-problem, and per-step isolation boundaries.
TASK-001 deliberately does not contain the later server Workflow-run route,
tenant-qualified binding resolver, public WyrdGateway caller, or server
authorization path; those require their owning later-task journey evidence.
This is a task boundary, not a verification limit on implemented TASK-001
behavior.

## Material proposed findings

None.

## Overall result

**PASS**

The final candidate preserves the single screened provider/ExtGateway
transport, exact runtime credential binding, fail-closed credential reflection
controls, safe remote-problem projection, terminal security classification,
and per-attempt request isolation. The final remediation introduces no
task-scoped network-security defect.
