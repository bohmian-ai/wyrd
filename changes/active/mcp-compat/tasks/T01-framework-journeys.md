---
task: T01-framework-journeys
spec: SPEC-mcp-compat r2
obligations: [REQ-003, INV-001, INV-002, INV-003, AC-001, AC-002, AC-003]
depends_on: []
skills: [$wyrd-implement]
---

# T01 Prove Wyrd MCP with the reference client and three Python agent frameworks

## Outcome and Value

The REQ-003 server repairs are done (`f995a88dc` cache metadata, `3ec0fc646`
object-rooted schemas). What remains is proof from real clients:

- The TypeScript journey proves only what the official MCP SDK reference
  client can prove.
- New Python journeys prove that the OpenAI Agents SDK (strict conversion),
  Pydantic AI, and Google ADK each convert the full catalog, hide write tools
  from a read-only caller, surface a hidden-tool refusal as a tool error,
  invoke `wyrd.guide` through their own tool path, and forward only
  provider-valid tool names.
- The framework journeys run in their own lane with their own dependency
  group.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Catalog schema-root rule | `crates/wyrd/wyrd-server/src/mcp/mod.rs` `tests::every_tool_schema_is_accepted_by_common_tool_callers`; `discovery.rs` `pg_tests::agent_discovers_mcp_guide_and_sql_examples` | Both added in `3ec0fc646` | None; this owns AC-002 | Keep as-is | None |
| Reference client journey | `sdks/wyrd-sdk-ts/wyrd/tests/integration/mcp-framework-compatibility.test.ts` | `serverTest()` fixture in `tests/support/server.js` | Repeats the schema rule with `ROOT_COMBINATORS`; hard-coded `WRITE_TOOLS`; carries the OpenAI JS half | Trim in place, then rename to reflect the reference-client role | None |
| Python real-server journeys | `sdks/wyrd-sdk-python/tests/integration/conftest.py` session `WyrdTestServer`; `test_mcp_host_connection.py`; `test_principal_roles.py` (scoped principals) | `py:test:integration:inner` (`-m 'integration and not identity and not local_dev'`) | No framework journeys | New journey file using the existing session server and principal fixtures | None |
| Lane isolation | `pyproject.toml` `[dependency-groups]`, pytest `markers`; `mise.toml` `py:test:integration*` | `local_dev` / `identity` marker exclusion precedent | Framework deps would otherwise enter `dev` and the default lane | New dependency group, marker, and mise lane following the `local_dev` precedent | Separate lane required by spec constraint (deps isolated from the fast lane) |

## Owners, Scope, Consumers, and Prohibited Changes

- **Owners:** TS SDK integration tests, Python SDK integration tests, Python
  project dependency groups, and `mise.toml`.
- **Production code:** expected to stay unchanged. If a framework exposes a
  real server defect (for example, an output schema ADK cannot convert), fix
  it at the shared tool builder in `crates/wyrd/wyrd-server/src/mcp/`, never
  per tool and never in a client.
- **Prohibited:**
  - renaming any tool (spec non-goal);
  - adding a framework package to `wyrd-ai` runtime dependencies or to the
    `dev` group;
  - re-adding hand-written schema-root assertions in client journeys
    (AC-002);
  - skipping a test to make a lane pass (`importorskip` included);
  - model credentials or live provider calls;
  - MCP resources, prompts, or sampling.

## Approach

1. Trim the TS journey to the official MCP SDK reference client and rename
   the file to name that role. Remove the `@openai/agents-core` dev dependency
   and its lockfile entries.
2. Add a Python dependency group for the three frameworks, a pytest marker,
   and a `mise` lane that installs the group and runs only marked journeys.
   Exclude the marker from `py:test:integration:inner`.
3. Write one Python journey file with top-level tests: per-framework
   conversion and invoke, read-only catalog, hidden-tool refusal, and provider
   name rule. Use offline or test models where a model loop is needed.
4. Run each journey with tracing on and record which, if any, fail before a
   fix. A server-side defect gets the root fix at the shared builder.
5. Point the MCP tool reference docs, where they name compatible clients, at
   the proved set.

## Ordered Implementation Scenarios

### Scenario 1 — Reference client proves negotiation, full catalog, and output validation

**Behavior.** The official `@modelcontextprotocol/client` connects with
version discovery, lists tools as an administrator, and calls `wyrd.guide`.
The SDK's own output-schema validation accepts `structuredContent`.

"Full catalog" means the listed names equal the names the server advertises
to an administrator, compared against a server-derived source (for example, a
raw `tools/list`), not a hard-coded subset.

There are no `ROOT_COMBINATORS` loops. Maps to REQ-003 bullet 3 and AC-002.

**RED.** Not a new behavior. The trimmed test must still fail if the
cache-metadata fix regresses. Prove that by temporarily removing `ttlMs` from
the server's `tools/list` result, observing the SDK rejection, and restoring
it. Command:
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/mcp-reference-client.test.ts'`
(after `mise run ts:build && mise run ts:build:testing`).

**GREEN.** Restore the server and the test passes. Delete the OpenAI Agents
JS test and its dev dependency.

**REFACTOR.** Keep the single test short, with one module doc sentence stating
that it is a server conformance journey through the reference client and does
not exercise `wyrd-sdk-ts`.

### Scenario 2 — OpenAI Agents SDK converts every tool under strict schemas

**Behavior.** The `openai-agents` Streamable HTTP MCP server, authenticated as
administrator, converts tools with strict-schema conversion enabled.

- The converted count equals the server's advertised count.
- Invoking the converted `wyrd.guide` tool returns the overview guide.

**RED.** Write the journey. If it passes immediately, prove it is sensitive:
temporarily restore a root `oneOf` on `operator_connections.create` input,
observe strict conversion failing or dropping the tool, then revert. Command:
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run --group mcp-frameworks python -m pytest -q -m mcp_frameworks "tests/integration/test_mcp_agent_frameworks.py::test_openai_agents_converts_every_tool_strictly"'`

**GREEN.** Root-fix any server defect at the shared builder. Otherwise no
production change.

**REFACTOR.** Share only connection and authentication setup across the
framework tests. Keep each framework's assertions in its own test.

### Scenario 3 — Pydantic AI runs an agent loop that calls a Wyrd tool

**Behavior.** A Pydantic AI agent uses Wyrd's MCP server as a toolset with an
offline test model restricted to calling `wyrd.guide`. The run completes with
the guide's result in the tool return, and the toolset exposes the full
advertised catalog.

**RED.** Same as Scenario 2. Command:
`... python -m pytest -q -m mcp_frameworks "tests/integration/test_mcp_agent_frameworks.py::test_pydantic_ai_agent_calls_wyrd_guide"`
(the same wrapper as Scenario 2).

**GREEN.** As Scenario 2.

**REFACTOR.** As Scenario 2.

### Scenario 4 — Google ADK converts every tool to Gemini declarations

**Behavior.** ADK's `McpToolset` over Streamable HTTP lists all advertised
tools. Each tool produces a Gemini function declaration without error, and
invoking `wyrd.guide` through the ADK tool returns the guide.

This is the most likely scenario to expose a real defect, because Gemini
accepts a different JSON Schema subset than OpenAI (for example `$ref`/`$defs`
handling).

**RED.** Same pattern. Command:
`... python -m pytest -q -m mcp_frameworks "tests/integration/test_mcp_agent_frameworks.py::test_google_adk_declares_every_tool"`

**GREEN.** A server schema shape ADK rejects is fixed at the shared builder
(for example, inlining definitions) while the Rust catalog test and discovery
journey stay green. A rejection caused by an ADK bug that Wyrd cannot avoid
without a contract change is a stop condition.

**REFACTOR.** As Scenario 2.

### Scenario 5 — Read-only callers see no write tools, and hidden writes refuse as tool errors

**Behavior.** For each framework, a principal without write permissions gets
a converted catalog with no write tool. Calling `verification.execute` (or
another hidden write) through the framework's generic call path returns a
tool error carrying a Wyrd problem code. It is never a success and never an
unhandled client exception that tears down the session. Maps to REQ-003
negative bullets and INV-002.

**RED.** Command:
`... python -m pytest -q -m mcp_frameworks "tests/integration/test_mcp_agent_frameworks.py::test_read_only_caller_sees_and_reaches_no_write_tool"`
(parametrized over frameworks).

**GREEN.** No production change is expected. A framework that crashes
instead of surfacing a tool error is a framework-side finding to record. If
Wyrd's error shape causes it, fix the shape at the server.

**REFACTOR.** Reuse the existing scoped-principal pattern from
`test_principal_roles.py`. Do not add a new token-minting helper.

### Scenario 6 — Forwarded tool names satisfy the provider rule

**Behavior.** For each framework, every name it would send to a model
provider matches `^[a-zA-Z0-9_-]{1,64}$`.

**RED.** Command:
`... python -m pytest -q -m mcp_frameworks "tests/integration/test_mcp_agent_frameworks.py::test_forwarded_tool_names_are_provider_valid"`

**GREEN.** If every framework sanitizes the names, pass. If one forwards
dotted names, stop (see Material Stop Conditions); do not rename tools.

**REFACTOR.** None beyond Scenario 2.

## Acceptance Criteria

- The TS reference-client journey passes, is trimmed and renamed, and
  `@openai/agents-core` is gone from the TS manifest and lockfile.
- Python framework journeys pass for all three frameworks, covering
  conversion, invoke, read-only catalog, hidden-write refusal, and the name
  rule.
- The default Python integration lane neither collects nor needs the
  framework dependencies. The new lane runs them against repository-managed
  Postgres.
- The Rust catalog unit test and discovery journey still pass, unchanged
  unless a server root fix required it.
- No model credentials and no live provider calls.
- If production code changed, the MCP tool reference docs reflect the change.

## Expected Write Set and Consumer Closure

- `sdks/wyrd-sdk-ts/wyrd/tests/integration/mcp-framework-compatibility.test.ts`
  becomes `mcp-reference-client.test.ts`.
- `sdks/wyrd-sdk-ts/package.json` and `pnpm-lock.yaml`
- `sdks/wyrd-sdk-python/pyproject.toml`: dependency group and marker
- `sdks/wyrd-sdk-python/uv.lock`
- `sdks/wyrd-sdk-python/tests/integration/test_mcp_agent_frameworks.py`
- `mise.toml`: the new lane, and the marker exclusion in
  `py:test:integration:inner`
- Only if a defect is found: `crates/wyrd/wyrd-server/src/mcp/mod.rs`, with
  `docs/src/content/docs/reference/mcp-tools.md`

## Verification and Evidence

- The exact commands in Scenarios 1–6, run with `WYRD_LOG=info` on any
  failure.
- `mise run py:format` and `mise run py:lints`
- `mise run ts:format:check`, `mise run ts:lints`, and `mise run ts:typecheck`
- The new framework lane, run as a whole.
- If server code changed:
  - `mise run fmt` and `mise run lints`
  - `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=mcp::tests::every_tool_schema_is_accepted_by_common_tool_callers)'`
  - `mise run test:bifrost:journey:mcp`
  - `mise run codegen:check`

## Material Stop Conditions

- A covered framework forwards dotted tool names to providers. Tool naming is
  a public contract and needs a spec revision.
- A framework rejects a Wyrd schema because of a framework bug that Wyrd can
  only work around by changing a public tool contract.
- A framework cannot run without model credentials or a live provider.
- A framework does not support the Python versions the repository's Python
  lanes run (including 3.14). Decide lane Python pinning with the owner rather
  than adding environment markers that silently skip the framework.

## Authority Links

- `changes/active/mcp-compat/spec.md` (r2)
- `AGENTS.md` §2, §11, §12, §16
- `architecture/agent-rules.md`
- `TESTING.md`
- `docs/src/content/docs/reference/mcp-tools.md`
