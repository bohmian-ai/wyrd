# Network-security domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `afdd8cd716c4529bd8cbb7fbe175bef55ef6ee1f`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 11 at `9a621a28a`
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Prior review and remediation: `review/TASK-001-r2/`, `TASK-001-R1-close-validated-runtime-gaps.md`, and `TASK-001-R1-addendum-revision-11.md`

The candidate remained at the stated commit throughout this audit. No
`.codegraph/` index exists, so source and caller tracing used repository search
and the complete immutable diff.

## Reviewed boundary and coverage

| Boundary | Authority and source traced | Result |
|---|---|---|
| Declarative route admission | REQ-042, INV-010, AC-016; `wyrd-spec/src/card/workflow.rs:497-632`; `skald-workflow/src/plan.rs:53-145,221-305` | PASS: pure validation rejects userinfo/query/fragment, invalid or duplicate authored headers, authored credential headers, and protocol/request mismatch before planning. |
| Runtime binding admission | REQ-042 and prior FIND-TASK-001-17; `skald-workflow/src/route.rs:95-198` | PASS: bindings are runtime-only, redact values in `Debug`, require a bare origin, validate values, and reject the shared routing/framing/forwarding/proxy/internal header class before insertion. Credential headers remain binding-owned. |
| Exact binding and dispatch | `skald-workflow/src/route.rs:264-345,381-435,463-503,569-606`; all `ExternalGatewayBinding(s)` and `ExternalGatewayClient` callers | PASS: the named binding fixes protocol and normalized origin; authored/secret collisions fail; secret values become sensitive `HeaderValue`s; every ext-gateway model call uses the checked private per-attempt adapter. |
| Endpoint security owner | REQ-049, INV-010/017; `skald-providers/src/endpoint.rs:1-216`; relocation from `wyrd-gateway/src/endpoint.rs`; gateway consumers in `wyrd-gateway/src/adapter/http.rs:51-110` and server boot at `wyrd-server/src/boot/mod.rs:1464-1476` | PASS: one lower-level owner screens literal and every resolved address, rejects mixed blocked DNS answers, preserves TLS verification, disables proxies and redirects, and applies bounded DNS/connect/request/response behavior. The Wyrd gateway and production boot consume that owner; no second endpoint-policy implementation remains. |
| Error and diagnostic containment | REQ-042, AC-016, prior FIND-TASK-001-18; `skald-providers/src/clients/external.rs:134-173`; `skald-providers/src/error.rs:8-176`; Workflow projection in `attempt.rs` and `workflow.rs` | PASS for refusals: external-only status mapping preserves status/retry metadata while replacing the complete refusal body with a fixed diagnostic. `Display`, derived `Debug`, Workflow errors, and retry classification cannot expose a reflected bound value. Native-provider status behavior is unchanged. |
| Successful response containment | REQ-042 and AC-016; `skald-providers/src/clients/external.rs:96-127,145-173`; `skald-agent/src/loop_runtime.rs:297-376`; `skald-workflow/src/workflow.rs:493-540` | FAIL: a valid successful native response is decoded and returned without checking whether its payload reflects a bound secret, then Agent text and Workflow result projection retain it. NET-R3-001. |
| REQ-053 telemetry replacement | `skald-workflow/src/workflow.rs:197-221,416-490,583-633`; `skald-agent/src/loop_runtime.rs:297-317,396-404,590-635`; `skald-runtime/src/dispatch.rs:14-70`; deletion and consumer search across workspace/package/docs/examples/check scripts | PASS: the Skald Observer crate and Agent/Workflow/Python hooks are deleted without aliases. Workflow/Agent spans contain identifiers, counts, statuses, finish reasons, usage counts, and stable error codes; prompts, inputs, request/response bodies, tool arguments/results, and credentials are absent. The remaining unrelated Vala/Forge/UI uses of the word “observer” are outside the deleted Skald system. |

## Prior-finding closure

- **FIND-TASK-001-17 is closed.** `ExternalGatewayBindings::insert` applies
  `is_reserved_transport_header` to normalized `HeaderName`s, and the focused
  test covers host, framing/hop-by-hop, forwarding, proxy, and Wyrd-internal
  names while retaining ordinary credential names.
- **FIND-TASK-001-18 is closed.** `ExternalGatewayClient::post` sanitizes every
  final `ProviderError::Status` body at the external boundary, preserving the
  status and `Retry-After` metadata. The focused test proves complete error
  `Display`/`Debug`, Workflow projection, and terminal 401 classification omit
  the canary.
- **REQ-053 is satisfied in this domain.** The observer system is absent and
  the replacement tracing proof checks input, output, and tool-argument
  canaries across captured span/event attributes.

## Security Audit

### Critical

None.

### High

- **NET-R3-001 — INCORRECT** —
  [`crates/skald/skald-providers/src/clients/external.rs:96`](../../../../../crates/skald/skald-providers/src/clients/external.rs),
  [`crates/skald/skald-agent/src/loop_runtime.rs:346`](../../../../../crates/skald/skald-agent/src/loop_runtime.rs),
  [`crates/skald/skald-workflow/src/workflow.rs:534`](../../../../../crates/skald/skald-workflow/src/workflow.rs):
  successful external-gateway responses can disclose bound credentials.
  REQ-042 says a secret value must never enter a returned result, and AC-016
  requires external-gateway failures to close without credential disclosure.
  The candidate sanitizes only non-success `ProviderError::Status`; a 2xx body
  is decoded unchanged into `ProviderResponse`, its assistant content becomes
  `AgentRun.output`, and `AttemptOutcome::from_agent` retains it in the public
  `WorkflowRun`. A compromised or malicious bound gateway can therefore return
  a valid native response such as assistant text `bad credential: s3cret`; a
  workflow invoker who cannot read the runtime binding receives the credential
  in the run result and can reuse it against the protected upstream. This path
  is reachable through the ordinary local ExtGateway route and becomes more
  consequential for the approved tenant-qualified server consumer.

  The smallest correction belongs in `ExternalGatewayClient`, the sole owner
  that has both the sensitive bound header values and the bounded successful
  response before provider-specific decoding. Refuse a successful external
  JSON answer with one fixed safe provider error when any decoded JSON string
  or member name contains an exact bound secret; do not return or include the
  offending body. Use the existing sensitive-header designation and bounded
  body, leave native-provider clients and Wyrd-gateway response semantics
  unchanged, and do not add a second route-layer guard. Focused closure proof:
  make a bound mock gateway return a valid 200 response whose assistant content
  reflects a canary credential, then prove the complete client error and
  serialized Workflow result omit the canary and that no retry occurs for the
  containment refusal; retain the existing 401 reflection test.

### Medium

None.

### Low / Defense In Depth

None. Optional hardening outside the approved task was not reported.

### Positive Controls

- Secret values use `SecretString`; binding and dependency `Debug` output only
  names/capability presence, and merged secret headers are marked sensitive.
- Exact origin/protocol matching occurs before client construction, and Card
  headers cannot supply credentials or collide with bound names.
- DNS answers are fail-closed as a set, connected through the screened
  resolver, with metadata/link-local denial in every profile and internal-range
  denial in production.
- TLS verification remains enabled; environment proxies and redirects are
  disabled; resolution, connection, request, and response sizes are bounded.
- External refusal bodies are withheld at the narrow external-client boundary,
  preserving native-provider diagnostics and retry metadata.
- Telemetry records stable codes and non-payload identifiers rather than
  prompts, bodies, arguments, results, or credential values.

## Verification evidence and limits

Executed against the immutable candidate:

- `workflow::tests::bound_external_gateway_security`: **1/1 passed**.
- `skald-providers` `endpoint::tests::*`: **4/4 passed** (address/profile
  policy, blocked hostname and redirect refusal, TLS certificate refusal).
- `workflow::tests::run_tracing_spans`: **1/1 passed**.
- `skald-agent` `agent_run_emits_genai_spans_without_payloads`: **1/1 passed**.
- Explicit base-to-candidate `git diff --check`: **passed**.

The focused security test proves reflected credentials only on the non-success
path. It has no successful 2xx reflection case, and the source trace above
shows the unchecked producer-to-result path. This review did not repeat every
recorded broad remediation lane; those results remain supporting evidence, not
proof of NET-R3-001.

## Overall result

**FAIL**

FIND-TASK-001-17 and FIND-TASK-001-18 are closed and REQ-053 telemetry is
payload-free, but NET-R3-001 leaves the explicit REQ-042 returned-result secret
containment invariant unsatisfied.
