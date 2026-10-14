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

## Implementation Evidence

Status: `SPEC_REVISION_REQUIRED` for Google ADK and for provider names on
Pydantic AI. Everything else is implemented in `a87c1cb4b`, `c6f53481f`,
`4386a3ede`, and `06750ca55`.

### Stop conditions reached

1. **Google ADK cannot connect.** `google-adk` 2.12.0 opens every session
   with the legacy `ClientSession.initialize()`
   (`google/adk/tools/mcp_tool/session_context.py:62`). Wyrd serves only
   2026-07-28, which is reached through `server/discover`, so ADK fails with
   `MCPError: Unsupported protocol version` and runs with no Wyrd tools.
   Serving the older lifecycle is a SPEC non-goal. ADK was removed from the
   dependency group because no journey can use it.
2. **Dotted names reach providers.** Pydantic AI 2.56 offers the model the
   raw MCP names (`bifrost.list_tables`, `wyrd.guide`), which OpenAI rejects.
   The OpenAI Agents SDK does the same by default and sanitizes names only
   with `mcp_config={"include_server_in_tool_names": True}`
   (`mcp_wyrd__wyrd_guide`). The journeys use that option. Tool naming is a
   public contract, so no rename was made.

### Findings fixed at the server

- **Gateway writes were advertised to readers** (REQ-003, read-only caller).
  Found by the reader journey: a viewer was shown all ten gateway
  put/revoke/reset/delete tools, each with `readOnlyHint: false`. Root fix in
  `crates/wyrd/wyrd-server/src/mcp/gateway.rs`: each tool carries the
  permission its operation enforces. Reads stay in the always-advertised
  catalog, and `write_descriptors(caller)` lists a write only when the
  caller's permissions contain it. This is the rule principals, verification,
  and operators already follow. Docs updated in `mcp-tools.md`.
  - Diagnosis of the follow-on failure in
    `connectivity::pg_tests::streamable_http_client_reaches_authenticated_server_context`:
    - Symptom: catalog order mismatch.
    - Cause: the test pins list order, and gateway writes now follow the
      caller's other write tools.
    - Fix site: that test's expectation. The `discovery.rs` constant was
      reordered to match.

### Recorded, not changed

- **Strict conversion falls back for 8 of 35 tools.** The OpenAI Agents SDK
  converts every tool (none dropped) but keeps 8 non-strict:
  - `verification.execute` has free-form `context` maps, which strict mode
    cannot represent.
  - `cards.list`, `cards.get_by_ref`, four `gateway.put_*` tools, and
    `operator_connections.update` keep bare `$ref`s. The SDK refuses those
    under MCP (`_UNVALIDATED_REF_ERROR`). Inlining definitions at the shared
    builder would make the seven strict. The spec requires conversion with
    strict enabled, not strictness for each tool, so this is left as a
    recommendation.
- **Refusal shape.** A hidden write is refused before the call does anything
  (`WYRD_PERMISSION_403_DENIED_RBAC`) and comes back as a JSON-RPC error.
  - Pydantic AI hands it to the model as `ModelRetry`.
  - The OpenAI Agents SDK's generic `call_tool` raises `MCPError` carrying the
    code, and the connection stays usable for the next call.
  - The arguments must be valid UUIDv7 ids. With malformed ids, input
    validation answers first.

### Deviations from task text

- One Python file, at `tests/integration/mcp_frameworks/test_agent_frameworks.py`.
  The lane is isolated by `norecursedirs` plus an explicit path rather than a
  pytest marker: a marker deselects tests only after the module is imported,
  and importing it needs the framework group. The directory is excluded from
  `ty` for the same reason.
- The TS journey proves the full catalog without a hard-coded list. The SDK
  rejects the whole list if any tool or the result is malformed, and the test
  asserts the administrator's list has both read-only and write tools.
- The provider-name check is part of the OpenAI Agents journey.
  `test_forwarded_tool_names_are_provider_valid` was not written because two
  of the three frameworks are blocked above.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| TS reference journey trimmed, renamed; `@openai/agents-core` removed | `sdks/wyrd-sdk-ts/wyrd/tests/integration/mcp-reference-client.test.ts`, `package.json`, `pnpm-lock.yaml` | RED with `ttlMs` removed: `SdkError: Invalid result for tools/list: ["ttlMs"]`. GREEN after restore: 1 passed. | PASS |
| OpenAI Agents SDK converts every tool (strict on), invokes `wyrd.guide`, names provider-valid | `test_openai_agents_converts_every_tool` | `mise run py:test:mcp-frameworks` (4 passed) | PASS |
| Pydantic AI converts every tool and an agent loop calls `wyrd.guide` | `test_pydantic_ai_agent_calls_wyrd_guide` | same lane | PASS |
| Reader sees no write tool; hidden write refused with Wyrd code, not success or crash | `test_openai_agents_reader_sees_and_reaches_no_write_tool`, `test_pydantic_ai_reader_sees_and_reaches_no_write_tool`, `mcp/gateway.rs` | RED before the gateway fix (viewer listed `gateway.put_*`). GREEN after. | PASS |
| Google ADK journeys | none | ADK cannot connect (stop condition 1) | BLOCKED |
| Forwarded names provider-valid for every framework | OpenAI Agents with `include_server_in_tool_names` | Pydantic AI forwards dotted names (stop condition 2) | BLOCKED |
| Default lanes neither collect nor need the frameworks | `pyproject.toml` `norecursedirs`, `ty` exclude, `[tool.uv.dependency-groups]` | `pytest --collect-only` collects 0 framework tests; the explicit path collects 4 | PASS |
| Rust catalog test and discovery journey pass | `mcp/mod.rs` test also covers every gateway tool | `cargo nextest ... -E 'test(/^mcp::/)'` (3 passed); `mise run test:bifrost:journey:mcp` (19 passed) | PASS |
| No credentials or live provider calls | scripted `FunctionModel`, direct tool invocation | journeys run offline | PASS |
| Format, lints, codegen | — | `mise run fmt`, `mise run lints`, `mise run codegen:check`, `py:format`, `py:lints`, `py:typecheck`, `ts:format:check`, `ts:lints`, `ts:typecheck`, `git diff --check` all pass | PASS |

Non-goals stayed excluded: no tool renamed, no older protocol, no resources or
prompts, no framework in runtime or `dev` dependencies.
