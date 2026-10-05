# Network-security domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `a704a8890ef20efe65fee1e116f7d02288f8ec5c`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 11
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Remediation authority: `changes/active/skald-workflow-runtime/review/TASK-001-r3/TASK-001-R2-close-round-three-runtime-gaps.md`

The candidate remained at the stated commit throughout this audit. No
`.codegraph/` index exists, so the review used the complete immutable diff,
repository search, and direct caller/source tracing.

## Reviewed boundary and authority coverage

| Boundary | Authority and source coverage | Result |
|---|---|---|
| External route and credential binding | Revision 11 REQ-042, REQ-049, INV-010, INV-012, INV-017, AC-016; `crates/skald/skald-workflow/src/route.rs:95-198,264-345,463-503`; route validation in `crates/wyrd-spec/src/card/workflow.rs` and resolved-plan validation in `crates/skald/skald-workflow/src/plan.rs` | PASS: runtime-only bindings validate bare origins, reserved header classes, and header values; exact protocol and normalized origin are matched before dispatch; Card headers cannot replace bound secret headers; `Debug` output omits values. |
| Shared endpoint security owner and consumers | Repository SSRF rules in `architecture/agent-rules.md`; Revision 11 REQ-049 and INV-010/017; `crates/skald/skald-providers/src/endpoint.rs:23-216`; `crates/skald/skald-providers/src/clients/external.rs:68-144`; gateway consumer `crates/wyrd/wyrd-gateway/src/adapter/http.rs:51-120`; production composition `crates/wyrd/wyrd-server/src/boot/mod.rs:1464-1477` | PASS: the prior gateway policy was moved, not forked. Literal and resolved addresses are screened, any mixed blocked DNS answer refuses the lookup, the connection uses the screening resolver, redirects and proxies are disabled, TLS verification remains enabled, and DNS/connect/request/response work is bounded. Production composition selects the production policy. |
| Scheme/origin enforcement | Revision 11 ExtGateway trust contract and INV-010; `crates/skald/skald-workflow/src/route.rs:299-343`; `crates/skald/skald-providers/src/endpoint.rs:66-130` | PASS: route origin must equal the configured binding, production allows only HTTPS on port 443 to public addresses, and local HTTP is limited by the route owner to explicitly configured loopback origins. Metadata/link-local destinations remain blocked in every profile. |
| Failed-response secret containment | REQ-042, INV-012, AC-016; `crates/skald/skald-providers/src/clients/external.rs:146-195`; shared bounded read/retry in `crates/skald/skald-providers/src/clients/mod.rs` | PASS: final refusal bodies and decode diagnostics are replaced at the external-client boundary while status and `Retry-After` metadata remain available. Transport, timeout, and oversized-body errors carry no request header values. |
| Successful-response secret containment and prior `FIND-TASK-001-22` | REQ-042, INV-012, AC-016 and the round-three remediation; `crates/skald/skald-providers/src/clients/external.rs:196-235`; `crates/skald/skald-workflow/src/workflow.rs:2402-2632`; Agent/Workflow result consumers | PASS: the client retains the sensitive designation on bound values, re-encodes only the successfully decoded typed response, recursively checks retained string values and member names after JSON escape decoding, fails closed on re-encoding failure, and returns one fixed non-retryable error before Agent or Workflow result projection. The Workflow proof confirms one dispatch and no canary in the serialized run. |
| Gateway policy migration and dependencies | REQ-049/INV-017; cumulative manifest and lockfile diff; deletion of `wyrd-gateway/src/endpoint.rs`; `skald-providers/Cargo.toml` | PASS: no new production third-party package was introduced. Tokio's existing `net` feature enables the moved resolver; `rcgen` and `rustls` are existing workspace dev dependencies used only for TLS policy proof. Gateway dispatch and server boot consume `skald_providers::EndpointPolicy`; no second provider/ExtGateway SSRF implementation remains. |
| Test-harness fix `516d0fbcc` | `crates/wyrd/wyrd-testing/src/server.rs:185-242,745-774,3609-3638`; commit diagnosis and cumulative diff | PASS for this domain: the test-only owner now cancels the shared state token in both explicit and drop teardown, and declares the database fixture after runtime owners so it drops last. It changes no production authentication, authorization, credential, endpoint, or network policy and does not weaken a security control. |

## Prior-finding closure

**`FIND-TASK-001-22` is closed.** The earlier candidate sanitized only failed
ExtGateway responses. The current `ExternalGatewayClient::post` checks decoded
successful responses while it still owns both the sensitive bound header values
and the typed answer. Reflected values in assistant content, JSON-escaped text,
tool arguments, and retained structured member names are refused with a fixed
`SKALD_PROVIDERS_400_BAD_REQUEST`; that refusal is terminal and neither the
error nor serialized `WorkflowRun` contains the canary. Clean successful
answers, ignored unretained members, non-sensitive authored headers, and the
existing failed-response control remain accepted or sanitized as appropriate.

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None. No optional hardening outside the approved task is promoted to a finding.

### Positive Controls

- External credentials remain runtime-only `SecretString` values; binding and
  dependency diagnostics expose names and capability presence, not values.
- Bound headers are marked sensitive before transport and successful-response
  inspection uses that same designation instead of a second credential list.
- Exact binding protocol and origin checks happen before a client is built or
  any request is dispatched.
- Reserved routing, framing, forwarding, proxy, Wyrd-internal, and authored
  credential header names fail closed.
- DNS screening evaluates every resolved address and rejects the whole answer
  on one blocked address; the screened resolver is also the connection
  resolver, preventing an unchecked re-resolution path.
- Redirects, environment proxies, invalid TLS certificates, oversized bodies,
  and bounded network timeouts fail closed.
- Failed and successful external responses have distinct containment controls,
  both at the external transport owner before public error/result projection.

## Verification evidence and limits

Executed against the immutable candidate:

- `mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow::tests::external_gateway_success_reflection) | test(=workflow::tests::bound_external_gateway_security)'` — **2 passed**.
- `mise exec -- cargo nextest run --locked -p skald-providers --lib -E 'test(endpoint::tests::)'` — **4 passed**.
- `git diff --check a51af030b6039eea4b2914f3ebf2c31925d08721..a704a8890ef20efe65fee1e116f7d02288f8ec5c` — **passed**.

The focused checks directly cover the task's network-security and credential-
containment boundaries. The recorded `mise run test:wyrd` result after
`516d0fbcc` remains supporting evidence for the harness teardown correction;
this reviewer did not repeat that 2,000-plus-test lane. Server ExtGateway
configuration and tenant-qualified binding resolution belong to later tasks;
this review does not treat their absence from TASK-001 as a security finding.

## Material proposed findings

None.

## Overall result

**PASS**

The candidate closes `FIND-TASK-001-22`, preserves the single screened provider/
ExtGateway transport owner and its existing gateway consumers, and leaves no
reachable task-scoped network-security or credential-containment defect.
