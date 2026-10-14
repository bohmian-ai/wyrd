# T01 independent behavior review

Result: **FAIL**.

Subject: base `84c2af51b` to candidate `6fbd60f19163ea10689029982b809a158dbc4987`, approved SPEC-mcp-compat r2, and the original T01 task read from the base commit. HEAD still equaled the candidate at the end of source inspection. No source or dependency changes were made by this reviewer.

## Scope and evidence

Reviewed the cumulative diff, original task, approved spec, AGENTS.md, agent rules, spec-driven development, maintainer style, agent-harness and testing guidance, applicable Wyrd design/doctrine, the framework journey module, official TypeScript journey, gateway descriptor filtering and catalog consumers, dependency groups and lane selection, docs, and recorded verification. CodeGraph was attempted first for code navigation; its broad query returned unrelated results, so direct source inspection supplied the changed paths.

Traced the framework paths into the installed dependencies: ADK `McpToolset.get_tools` constructs `MCPTool` from the advertised descriptor, `McpTool` initializes `BaseTool.name` from the dotted MCP name, `_build_declaration` emits `self.name`, and invocation sends `_mcp_tool.name` to the server. Traced ADK's unknown-name execution through `flows/llm_flows/tools/_caller.py` and `_error_handler.py`. Traced OpenAI conversion through `agents/mcp/util.py`: enabling strict conversion can legitimately retain a non-strict fallback, so the spec's explicit requirement that conversion be enabled is not treated as a requirement that every resulting tool be strict.

Fresh orchestrator evidence in `verification.md` reports `WYRD_LOG=info mise run py:test:mcp-frameworks` passing six tests and `git diff --check` passing. Historical task evidence records Rust MCP, TypeScript, formatting, lint, codegen, typecheck and docs results. These green results establish the assertions executed, not missing assertions or compliance with non-goals. This reviewer ran no tests or builds.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-003 list result has `ttlMs: 0`, `cacheScope: private` | `mcp/mod.rs::list_tools` retains both metadata fields | Existing Rust journey and recorded TS sensitivity experiment | PASS |
| Full read/write catalog has accepted object-root input schemas and SDK-accepted outputs | Rust catalog test now chains all gateway descriptors; runtime catalog still uses generated schemas | Recorded Rust catalog/discovery passes; official TS guide output validation | PASS |
| TS official SDK negotiates the required revision and validates structured guide output | `mcp-reference-client.test.ts:17-29`, server allows only 2026-07-28 | Historical one-test pass; recorded cache metadata rejection/restoration | PASS |
| TS lists the full administrator catalog against a server-derived source | Test checks only that annotations include true and false; no independent names comparison | No assertion proves completeness; B-01 | FAIL |
| OpenAI own MCP client converts all tools with strict conversion enabled and invokes guide | `openai_tools`, `test_openai_agents_converts_every_tool`; count and guide result asserted | Fresh six-test pass | PASS |
| Pydantic AI own MCP client converts all tools and executes guide in an offline model loop | `pydantic_run`, equality of offered names and catalog, guide tool return | Fresh six-test pass | PASS |
| ADK own MCP client converts all tools and invokes guide | `adk_run` uses real connection and scripted runner; catalog equality and guide payload asserted | Fresh six-test pass, but custom adapter qualification applies below | PASS |
| Each framework offers no writes to a reader | Raw reader catalog requires read-only annotations; converted catalog matches count/names; gateway filtering removes writes | Fresh three reader test passes | PASS |
| OpenAI/Pydantic hidden-call refusal is a tool error with Wyrd code rather than success or crash | OpenAI generic `call_tool` catches exact permission-code MCPError and lists again; Pydantic public `direct_call_tool` catches ModelRetry with code | Fresh reader tests | PASS |
| ADK hidden-call refusal exercises Wyrd refusal through the framework generic path and carries Wyrd code | Unknown model name is rejected by ADK before any MCP invocation; assertion checks only one `error` key | Fresh pass proves only local unknown-name handling; B-02 | FAIL |
| Provider names satisfy rule through supported framework behavior without Wyrd-specific adapters | OpenAI has built-in qualified names; Pydantic has native `renamed`; ADK custom subclass rewrites dotted names | Fresh pass measures rewritten ADK names; B-03 | FAIL |
| INV-001 isolation and INV-002 auth, audit, secrets remain server-owned; advertisement grants no authority | Changed production path only filters catalog using already verified permissions; `get_tool` still resolves hidden writes and dispatch retains domain authorization | Existing MCP journeys recorded passing; no new durable writer or identity source | PASS |
| INV-003 `/mcp` statelessness | No session, affinity, durable connection or transport machinery added server-side | Static cumulative diff | PASS |
| Framework dependencies test-only and isolated from default install/fast collection | Separate dependency group; explicit journey directory; `norecursedirs`; dedicated setup lane | Recorded collection proof; fresh dedicated lane | PASS |
| AC-001 real server and framework public MCP paths, no model credentials/live provider | Session WyrdTestServer; real MCP transports; FunctionModel/ScriptedModel | Fresh six-test pass; ADK negative gap remains B-02 | FAIL |
| AC-002 schema-root rule remains owned once by Rust, not repeated in clients | Client tests contain no ROOT_COMBINATORS checks; Rust catalog/discovery retain coverage | Static diff and recorded Rust passes | PASS |
| AC-003 canonical lane and format/lint/codegen proof | Dedicated framework task included in Python verification/aggregate | Fresh framework pass; other checks recorded historically rather than rerun by reviewer | PASS |
| TS renamed/trimmed and OpenAI JS dependency removed | New reference-client file, removed old journey and package/lock entries | Static full diff | PASS |
| Non-goals: no resources/prompts/sampling, no new server auth path, no tool wire rename, no runtime framework dependency | Production diff only catalog permission filtering | Static full diff | PASS |
| Non-goals: no Wyrd-specific MCP adapter; stop when covered framework forwards dotted names | `ProviderNamedMcpToolset` replaces dots in copied ADK tools before declarations | Installed ADK producer confirms native dotted declarations; B-03 | FAIL |

## Proposed findings

### B-01 — MISSING: TypeScript completeness proof is absent

- Obligation: original task Scenario 1 explicitly defines full catalog as equality with a server-derived administrator catalog, not a subset.
- Location: `sdks/wyrd-sdk-ts/wyrd/tests/integration/mcp-reference-client.test.ts:24-26`.
- Producer-to-consumer evidence: `client.listTools()` yields the only list observed; the consumer reduces it to a set of boolean annotations. Any list containing one read and one write passes. For example, a client regression dropping 33 of the current 35 tools while retaining guide and one write would pass. SDK schema validation checks the shape of returned entries; it cannot prove omitted entries exist.
- Observable consequence: the claimed full-catalog reference proof does not guard tool omission, an explicit interoperability acceptance boundary.
- Smallest testable correction: retain the official client journey and compare its returned names with the administrator catalog obtained from a separate server-derived list source, as the original task requires. Keep schema-root validation in Rust. A missing-entry sensitivity check should fail while the ordinary journey passes.
- Sibling coverage: Python framework tests already use `advertised(...)` as an independent reference; Rust discovery pins its own complete runtime catalog. Neither substitutes for the required TS client comparison.

### B-02 — MISSING: ADK negative journey never receives the server refusal

- Obligation: original task Scenario 5 requires the hidden write to be called through the framework generic path and a tool error carrying a Wyrd problem code; AC-001 requires the client-to-server negative journey.
- Location: `sdks/wyrd-sdk-python/tests/integration/mcp_frameworks/test_agent_frameworks.py:338-346`.
- Producer-to-consumer evidence: viewer discovery correctly omits `verification.execute`; ScriptedModel emits `verification_execute`; ADK cannot find that name in its declared tool dictionary. Installed ADK `_caller.py:448-457` records a lookup error and `_caller.py:532-554` returns `build_tool_not_found_response` before tool execution. `_error_handler.py:80-99` manufactures an `error` payload naming available tools, with no Wyrd code. The test asserts only that its keys equal `["error"]`. It does not reach `MCPTool.run_async`/`session.call_tool` or Wyrd authorization.
- Observable consequence: ADK MCP refusal handling could crash on Wyrd's actual JSON-RPC permission error and this test would remain green. It proves ADK's unknown model-function handling instead of the required Wyrd interoperability boundary.
- Smallest testable correction: exercise an actual hidden-tool request under viewer credentials through an existing public ADK MCP invocation mechanism, and assert the exact Wyrd permission code is surfaced as a tool error with a usable lifecycle. Preserve the existing reader discovery check. If ADK cannot provide this public path under the approved contract, record that incompatibility and apply the task's stop process rather than substituting an unrelated local rejection. Do not add a second transport or authorization implementation.
- Sibling coverage: OpenAI's `server.call_tool` and Pydantic's `direct_call_tool` do reach Wyrd and assert its code; their success cannot prove ADK's response conversion.

### B-03 — VIOLATION / DRIFT: custom ADK renaming adapter bypasses the provider-name stop condition

- Obligation: spec Objective requires operation without Wyrd-specific adapters; non-goals prohibit an SDK-side MCP adapter. Original task Scenario 6 and Material Stop Conditions say to stop for a covered framework forwarding dotted tool names and obtain a specification revision before resolving naming behavior.
- Location: `sdks/wyrd-sdk-python/tests/integration/mcp_frameworks/test_agent_frameworks.py:174-188`, usage at `:212`, documented workaround in `docs/src/content/docs/reference/mcp-tools.md:29`.
- Producer-to-consumer evidence: installed ADK `mcp_toolset.py:537-550` constructs the native MCP tool; `mcp_tool.py:348-357` assigns its dotted name to BaseTool; `:397-405` emits `self.name` in the provider declaration. The candidate interposes a Wyrd-authored subclass which copies each object and replaces dots before the model sees it. The provider-name assertions therefore test custom adapter output, not covered framework behavior. Native prefixing cannot remove embedded dots. OpenAI's built-in include-server naming and Pydantic's built-in renamed toolset differ: those mechanisms are provided by the frameworks.
- Observable consequence: a user registering the endpoint with native ADK still receives dotted provider declarations contrary to the approved regex, while the journey claims compatibility using extra code the objective excludes. Documentation now directs consumers to implement that adapter themselves.
- Smallest correction boundary: remove the custom rewriting adapter and its compatibility claim unless a native framework configuration satisfies the approved name rule. If native ADK forwards dotted names, follow the original material stop condition: route the incompatibility to specification revision rather than renaming public Wyrd tools or adding an adapter under r2. This finding requires an authority decision if the desired result remains custom renaming; a bounded implementation task must not silently authorize it.
- Sibling coverage: `provider_name` is also used to configure Pydantic's existing public renaming mechanism and to express expected declarations; deleting those uses is not required merely because ADK's subclass is out of scope. Keep wire tool names unchanged.

## Complexity and regression assessment

Gateway advertisement filtering repairs the demonstrated reader defect at the server owner rather than filtering each framework's list. It reuses existing verified principal permissions and existing dispatch authorization; no replacement auth system is introduced. The extra descriptor helpers are local pure catalog views rather than durable owners. Reordering the two Rust catalog expectations follows the new read-then-write order without dropping catalog entries. No new transport, cache, persistence or credential service is justified or required by the discovered gaps.

No optional improvements are proposed. In particular, the eight non-strict OpenAI conversion fallbacks are not independently classified as a defect under r2's wording, and dependency directory isolation is a valid implementation choice despite the original marker suggestion.

Skipped: independent build, test, codegen and external-provider execution; the orchestrator owns execution and supplied the fresh framework result. Remaining risk: green framework tests currently overstate TS completeness and ADK negative/name interoperability.
