# T01 immutable review subject

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/mcp-compat`.
- Base: `84c2af51b` (approved r2 spec and original T01 task, immediately before T01 implementation).
- Candidate: `6fbd60f19163ea10689029982b809a158dbc4987`.
- Complete diff: `git diff 84c2af51b 6fbd60f19163ea10689029982b809a158dbc4987`.
- Latest incremental change: `git diff 4cf4a9004 6fbd60f19`.
- Approved spec: `changes/active/mcp-compat/spec.md`, revision 2.
- Original task: `git show 84c2af51b:changes/active/mcp-compat/tasks/T01-framework-journeys.md`.
- Current task and available recorded execution evidence: `changes/active/mcp-compat/tasks/T01-framework-journeys.md`. Treat execution statements as evidence claims, not acceptance authority.
- No previous formal verdict, finding ledger, or remediation task exists in the active packet. The user mentions stop-hook feedback; only the recorded task evidence is available to establish its details.
- Working tree was clean and HEAD equaled candidate before review. Only review artifacts may be added; source stays immutable.

## Shared starting navigation map

This map is a starting point, not exhaustive coverage or a conclusion.

- Python framework journeys: `sdks/wyrd-sdk-python/tests/integration/mcp_frameworks/test_agent_frameworks.py`. Producers: advertised catalog and caller headers; consumers: OpenAI Agents SDK tool conversion/invocation, Pydantic AI FunctionModel loop, Google ADK ScriptedModel/InMemoryRunner loop. Key symbols: `openai_tools`, `pydantic_run`, `ProviderNamedMcpToolset.get_tools`, `adk_run`, six framework tests. Trace names, descriptors, calls, refusals, and resource lifetimes through the installed dependencies when needed.
- Existing Python owners: integration `conftest.py`, `support.py`, `test_principal_roles.py`, public `wyrd.testing.WyrdTestServer` and scoped credentials.
- Rust production: `crates/wyrd/wyrd-server/src/mcp/gateway.rs`: `gated`, `descriptors`, `descriptors_unscoped`, `write_descriptors`, dispatch to `GatewayAdministration`. `mcp/mod.rs`: `WyrdMcpHandler::read_descriptors`, `get_tool`, `list_tools`, catalog schema unit test. Compare sibling principal/operator/verification permission filtering and actual gateway operations.
- Rust journey consumers: `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/connectivity.rs` and `discovery.rs`, reordered expected catalog.
- TypeScript reference journey: removed `mcp-framework-compatibility.test.ts`, new `mcp-reference-client.test.ts`; existing server fixture, official MCP SDK output validation; `package.json` and `pnpm-lock.yaml` remove OpenAI JS dependency.
- Dependency/lane isolation: Python `pyproject.toml`, `uv.lock`; `mise.toml` new `py:test:mcp-frameworks` lanes and aggregate integration; pytest collection and ty exclusions.
- Documentation: `docs/src/content/docs/reference/mcp-tools.md`: permission visibility and framework settings.
- Required authority: `AGENTS.md`, `architecture/agent-rules.md`, reference router, spec-driven development, maintainer style, applicable agent-harness/testing/language/security/design references. CodeGraph exists: use it first for code navigation.
- Verification claims: task evidence records framework six-test lane, Rust MCP tests and 19-test journey, TS reference sensitivity experiment, formatting/lints/typechecks/codegen/docs. No independent review execution yet.

## Review coordination

Six independent discovery scopes: behavior, invariants, repository standards, maintainer, system resilience, and security/RBAC (material gateway catalog filtering plus negative-flow proof). Domain scope combines identity, permission visibility, authorization and tenancy because they share the caller boundary; persistent data and concurrency are not materially redesigned.

Each reviewer writes only its assigned report with apply_patch. Reviewers are not alone in the worktree and must preserve others' edits. Do not edit source or run Cargo/build/codegen lanes; orchestrator owns any executable verification to avoid overlapping shared-target processes. Read-only source/static inspection and installed dependency inspection are allowed. Expand and verify this map independently.
