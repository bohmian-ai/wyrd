# Security / RBAC domain review

Overall result: **FAIL** — the production visibility change preserves the
server trust boundary, but the ADK negative journey does not exercise the
required server refusal through ADK.

## Immutable subject and reviewed boundary

- Base: `84c2af51b`.
- Candidate: `6fbd60f19163ea10689029982b809a158dbc4987`.
- Approved authority: `changes/active/mcp-compat/spec.md` revision 2 and the
  original `T01-framework-journeys.md` at the base commit.
- Boundary: verified caller → per-caller MCP catalog → framework invocation
  → domain authorization → tenant-bound work and canonical audit. The review
  covers the complete cumulative change to that boundary, not only the latest
  incremental commit. HEAD remained the candidate during inspection.

## Authority and source coverage

| Concern | Governing authority | Inspected implementation / consumers | Assessment |
|---|---|---|---|
| Catalog visibility is not invocation authority | AGENTS.md §§2, 9, 11; agent rules; `wyrd-design.md` doctrine 18 and 20; security posture trust boundaries and authorization; `references/languages/agent-harness.md` | `mcp/gateway.rs` descriptor builders and complete dispatch; `mcp/mod.rs` catalog, get_tool, list_tools and call_tool; sibling principal/operator/verification filtering | PASS: changed code selects advertisements only; hidden names still resolve and dispatch to their existing owners. |
| Gateway read/write/delete permission parity | Spec REQ-003, INV-002; security posture authorization | `GatewayAdministration` operation permissions and `audited`; shared `PermissionSet::contains`; gateway HTTP route callers | PASS: seven reads retain read permission; put/revoke/reset operations match `gateway:write`; credential/deployment deletion matches `gateway:delete`. Wildcard coverage is retained by the existing `PermissionSet::contains` owner. |
| Caller and tenant attribution | Spec INV-001/002; security posture security principles, tenant isolation and audit integrity; agent rules | `WyrdMcpHandler::request_context`/`caller`; `Caller::from_authenticated`; `GatewayAdministration::audited`; `audit::authorize`/`authorize_recording_denial`/`audit_event`; `VerificationControl::execute` | PASS: tenant/principal/delegation attribution still comes from verified HTTP extensions. Gateway work acquires the caller's `TenantConn`; checks stage allowed/denied decisions through the existing outbox before protected work. No new tenant selector, token issuer, audit writer or SQL owner is introduced. |
| Hidden-write refusal across frameworks | REQ-003 negative bullet; original T01 scenario 5; AC-001; AGENTS.md §11 | All six Python framework tests, viewer fixture, OpenAI `call_tool`, Pydantic `direct_call_tool`, ADK tool lookup and `McpTool.run_async`/`_run_async_impl` | FAIL: OpenAI and Pydantic send the hidden write and assert the Wyrd denial code; ADK only tests an undeclared tool-name error before transport invocation. |
| Existing public behavior / docs | INV-002/003; agent-harness; doctrine public surfaces | Changed MCP documentation, Rust connectivity/discovery expectations, private zero-TTL list result, TS reference journey, dependency/lane diff | No additional security finding. Public tool names and operation ownership are retained; production change adds no session state, credential handling or secret-return path. |

Routing also included the task-review skill, reference router,
`spec-driven-development.md`, `maintainer-style.md`, and the doctrine summary.
CodeGraph was used before source searches to locate the MCP and administration
owners and callers. Installed framework source was inspected under
`sdks/wyrd-sdk-python/.venv/lib/python3.12/site-packages`; the lock records ADK
2.12.0, OpenAI Agents 0.23.1, and Pydantic AI Slim 2.56.0.

## Proposed finding

### SEC-01 — ADK's hidden-write test never reaches Wyrd authorization

- Classification: **MISSING**, blocking acceptance/proof gap.
- Violated obligation: REQ-003 requires each covered framework to surface a
  refused hidden-tool call as a tool error; original T01 scenario 5 explicitly
  requires its generic call path to return the Wyrd problem code. AC-001 and
  AGENTS.md §11 require a real client → server → client negative journey.
- Location:
  `sdks/wyrd-sdk-python/tests/integration/mcp_frameworks/test_agent_frameworks.py:338`
  (assertions at lines 343–346).
- Producer-to-consumer evidence: the viewer's catalog excludes
  `verification.execute`; `adk_run` supplies only that viewer catalog to
  `ProviderNamedMcpToolset`. Its scripted model nevertheless calls
  `verification_execute`. In installed ADK,
  `google/adk/flows/llm_flows/tools/_caller.py:83–99` cannot find the name and
  `:530–554` returns `build_tool_not_found_response` without executing any
  tool. The MCP transport call occurs separately in
  `google/adk/tools/mcp_tool/mcp_tool.py:685`, inside `_run_async_impl`, which
  this branch never reaches. The assertion accepts any single `error` field
  and does not require `WYRD_PERMISSION_403_DENIED_RBAC`. The candidate task's
  own recorded refusal evidence acknowledges that ADK stops before the server.
- Existing sibling paths: OpenAI reader test uses the framework's
  `server.call_tool` and checks `MCPError` for the denial code, then lists tools
  again; Pydantic uses `MCPToolset.direct_call_tool` and checks `ModelRetry`
  for the same code. The unchanged Wyrd path is
  `mcp_execute` → `VerificationControl::execute`, which checks
  `Permission::verifier_run` and stages the denied audit before target lookup.
- Observable consequence: all six tests can pass while ADK's actual handling
  of Wyrd's server refusal is broken. This is not evidence of an authorization
  bypass; it leaves the explicitly required framework/server refusal boundary
  unproved.
- Smallest correction boundary: extend the existing ADK negative journey to
  send the known hidden write through ADK's public MCP invocation machinery
  using the viewer's credentials, and assert its returned/caught tool error
  includes the Wyrd denial code and permits subsequent tool use. Preserve the
  separate viewer catalog assertion. ADK's existing `McpTool` and
  `MCPSessionManager` own the invocation and error conversion; reuse that
  machinery rather than adding a Wyrd transport adapter, changing server
  authorization, or making a write visible to the viewer. A public descriptor
  obtained with an authorized catalog can describe the call without granting
  the viewer any authority. No production permission change is required by
  this finding.
- Focused closure proof: run the exact ADK reader test against the real test
  server, demonstrate the server's `WYRD_PERMISSION_403_DENIED_RBAC` reaches
  the framework error path, and show a follow-up read succeeds. Run the
  dedicated framework lane after that correction.

## Verification and limits

The orchestrator's `verification.md` records a fresh canonical
`WYRD_LOG=info mise run py:test:mcp-frameworks` PASS: six tests in 5.11s.
That verifies the present assertions, including ADK's local name rejection;
it cannot close SEC-01. Historical task evidence records Rust MCP and other
gates, but those results were not independently rerun by this reviewer.

No tests, Cargo/build/codegen commands, live provider calls, external accounts,
or source mutations were performed by this reviewer. No broader security
redesign or unrelated existing debt is proposed.
