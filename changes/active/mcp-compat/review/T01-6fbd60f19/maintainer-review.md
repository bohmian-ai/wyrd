# Maintainer review — T01

Overall: **FAIL**. Two changed journey assertions do not prove the outcomes their names and task evidence claim. These are acceptance-proof gaps with concrete regression cost, not requests for optional restructuring.

## Immutable subject and authority

- Base: `84c2af51b`; candidate: `6fbd60f19163ea10689029982b809a158dbc4987`.
- Reviewed the complete cumulative diff, approved `changes/active/mcp-compat/spec.md` r2, original task from the base, and candidate execution evidence.
- Applied `AGENTS.md` §§2–6, 9, 11–12, 15–16; `architecture/agent-rules.md`; `architecture/wyrd-design.md` doctrine, client model, and runtime identity; `architecture/wyrd-doctrine.mdx`; references for spec-driven development, maintainer style, agent harnesses, and testing workflows.
- Used CodeGraph before expanding into focused source reads and caller searches. HEAD remained the candidate. Source was not modified.

## Changed-surface coverage

| Surface | Symbols, callers, and tests inspected | Maintenance assessment |
|---|---|---|
| Gateway catalog | `gateway.rs`: entire descriptor construction moved to `gated`, `descriptors`, `descriptors_unscoped`, `write_descriptors`, `Effect`, constants, and `WyrdMcpHandler::gateway_tool`; actual permission assignments on `GatewayAdministration` operations | One descriptor definition remains the owner. Selection is synchronous and request-local; invocation still delegates to the existing administration owner. No duplicate server lifecycle, transport, validator, or durable workflow was introduced. |
| MCP handler | `catalog`, `get_tool`, `list_tools`, `call_tool`, `every_tool_schema_is_accepted_by_common_tool_callers`; sibling principal, verification, and Operator discovery/dispatch patterns | New gateway filtering extends the established per-caller catalog workflow. Tool resolution retains hidden tools so dispatch can enforce authorization. No new state owner is warranted for pure descriptor construction. |
| Rust journeys | Full changed connectivity test, `ADVERTISED_TOOLS`, discovery setup/catalog checking, surrounding opt-in and guide workflows | Reordered assertions follow the read-then-write catalog without changing their test purpose. Exact named catalog and probe placement remain readable. |
| Python fixtures and reference discovery | New file module, constants, `adk_environment`, `mcp_url`, `admin_headers`, `viewer_headers`, `advertised`, `provider_name`, `openai_server`; existing integration `conftest.py`, `support.py` usage, MCP host and principal patterns | Reuses the existing real session server and viewer credential rather than adding a token issuer or server harness. Reference discovery uses the installed official MCP client. |
| OpenAI journey | `openai_tools` and both OpenAI tests; installed SDK `get_all_tools`/conversion and `MCPServerStreamableHttp.call_tool` path | Shared setup stays beside tests; strict conversion and framework-owned name qualification are visible. Negative proof reaches the server and checks a Wyrd code, then verifies the connection remains usable. |
| Pydantic AI journey | `pydantic_run`, its scripted model, both tests; installed `MCPToolset.direct_call_tool` error conversion | Uses the framework's existing `renamed` and `FunctionModel` facilities. One invocation owns its captured model state; no retained service abstraction is needed. Negative call checks the framework's retryable error and Wyrd code. |
| ADK journey | `ProviderNamedMcpToolset.get_tools`, `adk_run`, `ScriptedModel.generate_content_async`, nested `run`, both tests; installed `McpToolset.get_tools`, `McpTool.run_async` and `_run_async_impl`, flow tool lookup/error handling | Model script and toolset remain test-local, and the toolset closes in `finally`. The refusal test stops at ADK's undeclared-name guard; see MNT-002. The custom naming subclass is explicitly visible rather than hidden in a generic helper; whether this violates the approved adapter non-goal requires task/spec reconciliation by acceptance review. |
| TypeScript reference journey | Entire removed two-test file and new `mcp-reference-client.test.ts`, existing `serverTest` lifecycle and authentication | Reuses the real server fixture and official client. Removes duplicate root-schema checks and obsolete JS framework dependency. Full-catalog assertion is inadequate; see MNT-001. |
| Dependency and lane configuration | Python manifest/group, pytest collection and ty exclusions, new mise tasks and their aggregate callers; Python lock changes; TS manifest and complete lock deletion | Frameworks remain test-only and the explicit directory prevents import-before-deselection failures in default pytest collection. No runtime SDK export or generated declaration changed. Lockfiles record dependency resolution rather than a second handwritten API. |
| Documentation and evidence | MCP tool reference permission/settings changes; task implementation record and focused verification claims; independent `verification.md` | Docs identify framework configuration and experimental ADK negotiation. The recorded six-test pass proves current assertions, not their completeness. No public Python/TS declaration parity work is needed for test-only helper signatures. |

## Findings

### MNT-001 — MISSING: “full catalog” test accepts an arbitrary read/write subset

- **Changed location:** `sdks/wyrd-sdk-ts/wyrd/tests/integration/mcp-reference-client.test.ts:23–26`.
- **Rule:** Maintainer style, “Tests: prove an outcome a caller cares about”; original T01 Scenario 1 explicitly defines full catalog as equality against a server-derived advertised-name source; spec REQ-003 requires the reference client to list the full administrator catalog.
- **Evidence:** The only catalog assertion reduces every tool to its `readOnlyHint` and compares the set with `{true, false}`. The new test would pass if the SDK exposed only `wyrd.guide` plus one write tool, while silently losing the other advertised tools. SDK schema validation does not prove list completeness. The Rust discovery journey's exact-name assertion proves a different consumer and cannot detect TS-client conversion/drop behavior.
- **Concrete maintenance cost / consequence:** A future SDK change that drops tools can leave this conformance journey green while agents lose most capabilities. Its name, comment, and task record currently tell maintainers that coverage exists, obscuring that regression.
- **Smallest testable correction:** Keep the existing short official-client journey and compare its returned names with the same caller's server-derived advertised names, as the original task requires. Reuse the existing server/header setup and native request facilities; no new transport abstraction, hardcoded catalog, or schema-root validator is needed. A controlled omission on the client-observed list must fail the equality while the unchanged candidate passes.

### MNT-002 — MISSING: ADK refusal assertion never exercises the Wyrd refusal path

- **Changed location:** `sdks/wyrd-sdk-python/tests/integration/mcp_frameworks/test_agent_frameworks.py:338–346`, reached through `adk_run:211–238`.
- **Rule:** Maintainer style's outcome-based tests and `AGENTS.md` §11 discover → act → observe negative journeys; original T01 Scenario 5 requires the framework generic call path to surface a refused hidden write with a Wyrd problem code.
- **Evidence:** The viewer catalog excludes `verification.execute`. `adk_run` supplies only that catalog to ADK, then the scripted model calls `verification_execute`. Installed ADK `flows/llm_flows/tools/_caller.py::_get_tool` cannot resolve that name and the lookup-error branch returns `build_tool_not_found_response` before normal tool invocation. The candidate itself documents this at line 343. The final assertion checks only that the response has one `error` key. It does not invoke `McpTool._run_async_impl`'s `session.call_tool`, inspect a Wyrd code, or exercise ADK handling of the server's actual JSON-RPC refusal. By contrast, the sibling OpenAI and Pydantic tests call the hidden wire name through their generic MCP call APIs and match `WYRD_PERMISSION_403_DENIED_RBAC`.
- **Concrete maintenance cost / consequence:** Server refusal shape or ADK's MCP-error handling can regress into a client crash or incorrect success while this negative journey stays green. A maintainer following its name and the evidence table will incorrectly believe the server/framework rejection seam is protected.
- **Smallest testable correction:** Preserve the viewer-catalog assertion, then drive a well-formed hidden write through ADK's own MCP invocation capability under the viewer credential, and assert the resulting tool error includes the actual Wyrd denial code. Reuse ADK's existing tool/session owner and the existing server/credential fixture; do not add a Wyrd error translator or advertise hidden tools to the viewer to manufacture success. If the public framework path cannot satisfy the approved outcome, report the incompatibility instead of substituting an unknown-tool check. Focused proof must fail when the Wyrd denial ceases to be surfaced, and pass for the real refusal without an unhandled client failure.

## Reuse and complexity assessment

The gateway change compares favorably with the nearby `principals`, `verification`, and `operators` complete discovery → resolve → dispatch workflows: descriptor selection changes, while the existing authorization/audit/tenant owner continues to execute operations. `gated` does not retain resources and does not warrant a new struct. Its closed descriptor/permission table avoids separate per-tool filters.

Each framework necessarily has a different tool/model API. The new helpers use that installed framework's connection and model machinery; an additional generic test-runner abstraction would add cost without removing a shared durable workflow. The new dedicated dependency lane is required by the approved isolation constraint. No other optional abstraction or refactor is proposed.

## Non-blocking calibration note

`WyrdMcpHandler::catalog`'s rustdoc still describes gateway replacement/delete operations as part of the universally advertised group, although its body now includes only gateway reads. The module-level docs and user reference describe the new behavior correctly. This wording inconsistency alone does not determine this verdict.

## Verification limits

Independent orchestrator evidence records six passing framework tests and an unchanged candidate. Historical task evidence records the Rust, TS sensitivity, lint/typecheck/codegen, and docs results. This reviewer ran no tests, Cargo, build, or codegen commands, as assigned; source inspection and `git diff --check` only. No live model provider was used. The findings concern missing proof, not a claim that the production paths have already failed.
