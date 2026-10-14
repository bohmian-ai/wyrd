# Independent structured Ponytail validation — T01

Overall implication: **SPEC_REVISION_REQUIRED**. Four deduplicated findings
remain. Three have bounded corrections; the ADK naming incompatibility cannot
be closed under approved r2 by authorizing the custom adapter during review.

## Subject, inputs, and method

- Base: `84c2af51b`; candidate and inspected HEAD:
  `6fbd60f19163ea10689029982b809a158dbc4987`.
- Approved authority: `changes/active/mcp-compat/spec.md` r2 and original
  `T01-framework-journeys.md` read from the base, not its completion claims.
- Read all six discovery reports, `claim-comparison.md`, `subject.md`, and
  `verification.md`. There is no follow-up report and no prior formal FIND ID.
- Inspected the complete cumulative changed surface, both dependency-lock
  changes, task evidence, owning server catalog/dispatch, sibling permission
  projections, existing server/viewer fixtures, framework conversion and error
  consumers, and installed framework/native MCP implementations. CodeGraph
  was consulted before code navigation. Applicable authority includes
  AGENTS §§2–6, 9–12, 15–16, agent rules, approved spec/task precedence,
  maintainer style, agent-harness/testing guidance, Wyrd doctrine/client and
  identity boundaries, and security authorization/tenant/audit rules.
- This pass is independent source validation, not a vote on six FAIL reports.
  No source, dependency, or execution evidence was changed, and no tests,
  builds, codegen, or live providers were run. The only write is this report.

The installed dependency evidence is under
`sdks/wyrd-sdk-python/.venv/lib/python3.12/site-packages`, consistent with the
lock's Google ADK 2.12.0, OpenAI Agents 0.23.1 and Pydantic AI Slim 2.56.0.
TypeScript evidence uses the installed official MCP client 2.3.1.

## Source-local claim mapping

| Discovery proposals | Decision | Final identity |
|---|---|---|
| B-01, INV-03, MNT-001 | CONFIRMED | FIND-T01-1 |
| B-02, INV-02, MNT-002, SYS-001, SEC-01 | REVISED: retain gap, select the public ADK owner rather than leave the invocation choice unresolved | FIND-T01-2 |
| B-03, INV-01 | CONFIRMED: material compatibility decision remains; native aliases are distinguished from custom adaptation | FIND-T01-3 |
| STD-001 | CONFIRMED, bounded to the four cited changed items | FIND-T01-4 |

All independently proposed findings are accounted for. Duplicates are merged
by violated invariant, not multiplied by framework caller or report count.

## Final finding ledger

### FIND-T01-1 — TypeScript reference journey does not establish the full catalog

Status: **CONFIRMED**. Classification: **MISSING**. Sources: B-01, INV-03,
MNT-001. Owner: TS reference-client journey.

**Obligation and location.** REQ-003 reference-client bullet and original
Scenario 1 explicitly require listed names to equal the administrator's
server-derived catalog. In
`sdks/wyrd-sdk-ts/wyrd/tests/integration/mcp-reference-client.test.ts:24–26`,
the only catalog assertion compares the set of read-only hints with
`{true, false}`.

**Producer → consumer and consequence.** The existing `serverTest` fixture
provides the real server and administrator token. `Client.listTools()` is the
only observed list producer, and the assertion discards names and multiplicity.
A list retaining guide and one write tool passes while omitting the other
tools. The SDK validates returned values, but cannot establish equality with
an unobserved source. Installed `dist/index.mjs:4315–4331` also explicitly
routes aggregated results through its tool exclusion mechanism; schema
acceptance is not a general no-omission guarantee. This does not allege that
the current SDK actually omits a Wyrd tool. It proves the explicit completeness
regression boundary has no assertion. Python's `advertised` comparisons and
the Rust discovery exact-name catalog exercise sibling consumers and cannot
substitute for this TS obligation.

**Smallest correction.** Keep this test, fixture, official client and guide
output validation. Obtain an independent server-derived `tools/list` for the
same administrator using native request facilities, and compare the complete
sorted names with the official client's list. Replace the hint-set surrogate
with that equality. Do not add a hard-coded catalog, schema-root checks, a
second reusable transport, or a new dependency. The test is the source of the
invalid completion claim; production behavior does not need correction.

**Focused closure proof.** The existing exact TS file must pass against the
real server, and a temporary omission of one client-observed ordinary tool
must fail name equality even when both effect hints remain. Restore the
omission before the passing run. Exact command (after existing native setup):

```bash
WYRD_LOG=info scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/mcp-reference-client.test.ts'
```

### FIND-T01-2 — ADK hidden-write journey accepts a local lookup error

Status: **REVISED**. Classification: **MISSING**. Sources: B-02, INV-02,
MNT-002, SYS-001, SEC-01. Owner: Python ADK negative journey.

**Obligation and location.** REQ-003 negative bullet, AC-001, and original
Scenario 5 require a hidden-tool call to reach Wyrd through the framework's
invocation path and surface a Wyrd problem code without crashing the client.
The affected path is
`sdks/wyrd-sdk-python/tests/integration/mcp_frameworks/test_agent_frameworks.py:331–346`,
through `adk_run:191–238`.

**Producer → consumer and consequence.** `viewer_key` is the existing real
viewer principal; `advertised` obtains its correctly filtered list. ADK's
toolset is built from that list, so `verification.execute` is not declared.
The scripted model nevertheless produces `verification_execute`.
Installed `google/adk/flows/llm_flows/tools/_caller.py:83–101` rejects the absent
name; its prepared-call/error branch at `:449–457,529–555` returns
`build_tool_not_found_response` before running a tool. The producer of the
asserted `error` field is `_error_handler.py:80–99`, not the server.
`list(refused.response) == ["error"]` accepts this local result without a
Wyrd code. Thus a broken ADK MCP denial consumer would leave the test green.
No authorization bypass or deployed outage is claimed.

The actual required path is already owned by ADK:
`McpTool.run_async` → `_run_async_impl` →
`session.call_tool(self._mcp_tool.name, ...)` in
`google/adk/tools/mcp_tool/mcp_tool.py:569–625,649–690`.
The installed graceful-error feature is experimental but **default on**
(`features/_feature_registry.py:202–204`), and its native handler turns MCP
errors into an error dictionary. `MCPSessionManager` has a public constructor
accepting `StreamableHTTPConnectionParams` and a public `close` method. No new
error translator or transport is necessary.

On the server, `mcp/mod.rs::get_tool` retains hidden descriptors;
`mcp/verification.rs:181–194` decodes the call and delegates to
`VerificationControl::execute`; that owner validates input, checks
`Permission::verifier_run` and stages denial before target lookup
(`components/verification/service.rs:420–442`). The existing UUIDv7 arguments
therefore target authorization rather than an input-parsing refusal. Sibling
OpenAI `server.call_tool` and Pydantic `direct_call_tool` transmit this same
wire name and assert the denial code; they do not prove ADK's consumer.

**Selected bounded correction.** Preserve the ordinary reader discovery/name
assertion. For the explicit denied call, use ADK's public `McpTool` constructed
from the public administrator-discovered hidden descriptor, with a public
`MCPSessionManager` connected using **viewer** headers. Invoke the native tool
through its public `run_async`/existing scripted runner path; assert its error
contains `WYRD_PERMISSION_403_DENIED_RBAC`, then perform the guide read through
that same native connection owner to prove continued use. A known descriptor
describes a call and grants no authority. Keep the manager's existing close
lifecycle in `finally`. This direct native invocation proof does not require
putting a hidden tool into the viewer-discovered catalog or introducing a
model-name adapter. The naming policy for ordinary ADK model declarations is
separately FIND-T01-3 and must not be silently resolved here.

**Focused closure proof.** Run the corrected exact reader test with tracing
and assert the actual Wyrd denial code plus subsequent read; a tool-not-found
error must fail it. Then the existing six-test framework lane is the narrow
affected module lane, not a request for the broader integration/gate suite.

```bash
WYRD_LOG=info scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run --group mcp-frameworks python -m pytest -q tests/integration/mcp_frameworks/test_agent_frameworks.py::test_google_adk_reader_sees_and_reaches_no_write_tool'
mise run py:test:mcp-frameworks
```

### FIND-T01-3 — Custom ADK naming adapter bypasses the approved compatibility stop

Status: **CONFIRMED**. Classification: **VIOLATION / DRIFT**. Sources: B-03,
INV-01. Owner: Python ADK journey and MCP framework documentation. Disposition:
**SPEC_REVISION_REQUIRED**, not an implementer-selected compatibility policy.

**Obligation and location.** Approved r2's objective requires interoperability
without Wyrd-specific adapters; its non-goals exclude an SDK-side MCP adapter
and tool renaming. Scenario 6 requires stopping when a covered framework
forwards dotted names. The custom producer is
`test_agent_frameworks.py:174–188` (`ProviderNamedMcpToolset`), instantiated at
`:212`; the resulting compatibility recipe is published in
`docs/src/content/docs/reference/mcp-tools.md:29`.

**Producer → consumer and native alternatives.** Native ADK
`McpToolset.get_tools` (`mcp_toolset.py:537–550`) constructs each MCP tool from
the advertised descriptor. Native `McpTool.__init__` assigns
`BaseTool.name = mcp_tool.name` (`mcp_tool.py:348–357`), and
`_build_declaration` emits `self.name` (`:397–405`). The candidate copies these
objects and changes each `name` with Wyrd-authored dot replacement before
ADK builds the model request. Both ADK tests consume that subclass, so their
regex assertion proves custom adapter output. ADK still calls the original
wire name at `:685–690`, which explains the passing guide call.

The native constructor exposes `tool_name_prefix`, not a name mapping.
`BaseToolset.get_tools_with_prefix:148–180` copies tools and prepends
`f"{prefix}_{tool.name}"`; it preserves every embedded dot. Its existence does
not make a custom substitution override native configuration. Inspected ADK
toolset/tool and model-declaration paths reveal no installed native mapping
that removes those dots. A normal dotted `wyrd.guide` descriptor therefore
becomes a dotted model declaration without the candidate adapter.

By comparison, OpenAI's installed `MCPUtil.get_all_function_tools` and native
`include_server_in_tool_names` own qualification; Pydantic AI's installed
`AbstractToolset.renamed` and `RenamedToolset.get_tools/call_tool` own forward
and reverse name mapping. Those are framework-supplied model aliases and do
not rename Wyrd's public wire tools. They are permitted configuration under the
task's own framework-sanitization scenario and do not need deleting.

**Consequence and authority resolution.** A developer registering Wyrd with
native ADK still gets dotted model-facing declarations, contrary to r2's
unconditional regex. The test and docs claim the outcome by supplying custom
code the approved objective excludes. Keeping wire names unchanged is
necessary but does not waive the adapter constraint. Gemini accepting dots
also does not satisfy the explicit regex for every covered framework.

**Smallest permissible disposition.** Do not strengthen, move, or generalize
the subclass, and do not rename server tools. Native ADK without the subclass
exposes the already-specified material stop. Route that concrete conflict to
`wyrd-spec`: the owner must approve whether the ADK claim/name rule changes or
custom model-name adaptation becomes allowed (or another covered native
framework version is adopted with evidence). This report selects no such new
product/compatibility decision. Merely deleting ADK or its name assertion
would fail r2; merely deleting the subclass leaves the required grammar red.
After the decision, remove unauthorized adaptation/claims or update them only
as the new approved behavior permits. This is why an ordinary bounded fix task
cannot close the complete candidate under current r2.

**Closure proof.** Under unchanged r2, native complete ADK declarations must
satisfy the regex and guide must route to its original wire name without a
Wyrd-authored rewrite. The present installed native path cannot provide that
proof. A future approved revision must explicitly map the revised ADK naming
behavior and documentation to the existing exact administrator ADK journey:

```bash
WYRD_LOG=info scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run --group mcp-frameworks python -m pytest -q tests/integration/mcp_frameworks/test_agent_frameworks.py::test_google_adk_declares_every_tool'
```

### FIND-T01-4 — Changed Rust items omit required panic contracts

Status: **CONFIRMED**. Classification: **VIOLATION**. Source: STD-001.
Owner: MCP gateway descriptor projections and changed catalog unit test.

**Obligation and location.** AGENTS §16 explicitly requires substantive
rustdoc on changed/private/test items and `# Panics` whenever a panic remains
possible. Locations are `mcp/gateway.rs:323–353` (`descriptors`,
`descriptors_unscoped`, `write_descriptors`) and `mcp/mod.rs:439–470`
(`every_tool_schema_is_accepted_by_common_tool_callers`).

**Trace and consequence.** All three new projections immediately call
`gated`, whose schema parse reaches `unreachable!` at `gateway.rs:313` on an
invalid embedded object schema; `gated` documents that invariant but the new
entry points do not document or link its panic condition. Their actual
consumers are catalog construction, hidden-name resolution, per-caller list
construction and the catalog check (`mcp/mod.rs:154,195,225,450`). The changed
unit test directly asserts create-tool presence, schema roots and forbidden
root keywords without a panic section. This is the explicit repository
documentation completion gate, not an alleged runtime failure on the current
valid embedded schemas. Existing surrounding undocumented propagation does
not justify an unrelated documentation sweep.

**Smallest correction and proof.** Add concise panic contracts to those three
new projections referring to the embedded-schema invariant, and document
assertion failures on the changed test. Preserve parsing, schema assertions,
permissions, and all runtime behavior. Static comparison of these four docs
with their complete bodies plus normal `mise run fmt` is sufficient; no
malformed-schema harness, Result redesign, new check, or test execution solely
for documentation is required. Batch this with substantive correction work;
do not create a style-only review round.

## Acceptance-matrix reconciliation

PASS below means the obligation is supported at the reviewed scope, with
fresh versus recorded execution distinguished. It does not turn an adapted
ADK claim into native compatibility.

| Obligation | Source/evidence | Result |
|---|---|---|
| REQ-003 protocol 2026-07-28, private zero-TTL catalog | `mcp/mod.rs::list_tools`, stateless service; fresh TS reference pass and recorded TTL sensitivity | PASS |
| Full catalog object-root schemas and SDK-compatible output validation | Rust test includes every gateway descriptor; fresh exact Rust catalog test and TS structured guide pass; recorded Rust discovery | PASS |
| Official TS full administrator catalog | Client uses admin bearer but checks hint categories, not server-derived names | FAIL — FIND-T01-1 |
| Official TS structured guide output validation and negotiation | Official native SDK with auto negotiation and validated call result | PASS |
| OpenAI every-tool conversion with strict conversion enabled, guide invocation, provider names | Native strict/qualified-name configuration, count equality, converted tool call; fresh framework pass | PASS |
| Pydantic AI every-tool conversion, guide invocation, provider names | Native renamed toolset, full names equality, offline FunctionModel and tool return; fresh framework pass | PASS |
| ADK every-tool conversion and guide invocation | Full equality and native invocation succeed after a custom naming producer; fresh pass proves adapted path | FAIL for approved adapter-free claim — FIND-T01-3 |
| No writes exposed to reader, all three frameworks | Real viewer catalog has only read-only hints and converted list agrees; shared gateway permissions; fresh framework pass | PASS |
| OpenAI/Pydantic hidden-write refusal | Native generic calls reach Wyrd and assert its RBAC code; fresh framework pass | PASS |
| ADK hidden-write refusal | Local undeclared-name branch instead of actual MCP denial | FAIL — FIND-T01-2 |
| Every model-forwarded name satisfies regex through allowed mechanisms | OpenAI/Pydantic native aliases pass; ADK native dotted names hidden by custom override | FAIL — FIND-T01-3 |
| INV-001 tenant isolation | Catalog uses verified caller, existing tenant-bound operation owner unchanged; no new SQL/tenant selector | PASS |
| INV-002 auth/authz/audit/secrets unchanged; discovery grants no authority | Full hidden lookup retained; existing service authorization precedes tenant work; permissions match operations | PASS; ADK journey proof still fails separately |
| INV-003 stateless `/mcp` | Existing NeverSessionManager and request-local catalog; no new serving/session owner | PASS |
| Framework packages test-only and isolated from default development/fast lane | Separate group, explicit-path framework lane, collection directory exclusion; unchanged runtime requirements | PASS |
| AC-001 real server, framework public APIs, offline models and no model credentials | Existing real fixture and public native clients; ADK positive adapted and negative local paths limit claim | FAIL — FIND-T01-2/3 |
| AC-002 one Rust schema-root proof, no duplicate client root loops | Rust catalog/discovery retains owner; handwritten TS loops removed | PASS |
| AC-003 canonical framework lane plus owning checks | Fresh six framework, one TS and one Rust test pass; historical task records format/lint/types/codegen/docs passes | PASS with execution limits below |
| TS test trimmed/renamed and OpenAI JS package removed | Complete TS source/manifest/lock diff | PASS |
| Root-fix demonstrated production defect at shared owner | Gateway descriptor permission pairing; all seven reads and ten writes project existing permission owner; dispatch unchanged; Rust catalog order expectations updated | PASS |
| No second MCP/auth implementation, no wire rename, older protocol, resources/prompts/sampling or credential host config | Cumulative production diff has none; ADK adapter specifically excluded separately | PASS except FIND-T01-3 |
| No skip, credential-dependent provider call, or dependency marker that silently omits a covered lane | Explicit directory lane installs group; Python >=3.11 group floor; scripted models | PASS; no fresh Python 3.14 run claimed |
| Production/docs parity and changed Rust documentation gate | Gateway visibility prose matches behavior; framework adaptation claim unauthorized; panic contracts missing | FAIL — FIND-T01-3/4 |

The behavior/invariant acceptance matrices identify the same material gaps;
their ADK happy-path PASS rows are qualified by FIND-T01-3. The standards
report's broad docs-parity PASS means prose describes the implementation, not
that the described custom adapter is authorized. Its documentation FAIL is
retained explicitly. Maintainer and system/security reports add no independent
production authorization, availability, durability or recovery defect beyond
the consolidated negative proof gap.

## Rejected expansions and non-findings

- Reject treating OpenAI's eight non-strict fallback tools as another finding:
  r2 requires strict conversion **enabled**, not every output tool strict.
  Do not inline schemas or change free-form input contracts to satisfy an
  invented stronger requirement.
- Reject treating Pydantic's native aliases or OpenAI's native qualification as
  public wire renames. Their installed owners map invocations back to original
  MCP names; the prohibited custom ADK subclass is a different mechanism.
- Reject fixing the ADK negative case by changing Wyrd authorization, exposing
  writes to viewers, using a raw MCP call as the ADK proof, or adding a Wyrd
  error translator. Public ADK tool/session owners already provide that path.
- Reject treating ADK prefixing or Gemini accepting dots as closure of the
  unconditional regex. Neither removes the native dotted declaration.
- Reject a security-bypass or shared-server-availability finding from the ADK
  local error: the server authorization/lifecycle owners are unchanged; this
  is a required interoperability proof gap.
- Reject forcing a marker instead of explicit-directory collection: exclusion
  before import is necessary for missing optional packages and the dedicated
  lane is included in the existing aggregates. No skip weakens the proof.
- No optional catalog cache, descriptor abstraction, retry policy, outage
  harness, audit path, dependency, rustdoc sweep or broader aggregate is needed
  to close the validated bounded defects. The stale internal catalog prose
  noted by the maintainer is non-blocking wording, not a fifth finding.

## Verification and verdict implication

`verification.md` records fresh six-test Python framework, one-test TS
reference, and exact Rust catalog PASS, plus clean tracked source and diff
checks. These are credible executions of the current assertions. They do not
supply the missing TS equality, ADK server-denial consumer, or native ADK name
proof. Historical format/lint/type/codegen/docs/MCP journey results remain
implementation-recorded evidence; this validator did not rerun them or expand
their scope.

FIND-T01-1, FIND-T01-2 and FIND-T01-4 are bounded implementation/documentation
corrections. FIND-T01-3 invokes the original task's concrete material stop:
native installed ADK emits dotted model declarations and approved r2 excludes
the custom adaptation that masks them. Therefore the complete subject requires
**SPEC_REVISION_REQUIRED**, not PASS or an ordinary remediation task that
silently permits the adapter. No implementation changes are authorized by
this validation artifact.

Skipped independent execution and live providers; immutable HEAD remained the
candidate. Remaining risk is overstated TS completeness and ADK compatibility
until the validated proof gaps and naming decision are closed.
