---
id: T01-R1
kind: remediation
status: ready
spec: SPEC-mcp-compat
spec_revision: 3
requirements: [REQ-003, INV-001, INV-002, INV-003, INV-004, AC-001, AC-002, AC-003, AC-004]
depends_on: []
parent_task: T01-framework-journeys
remediates: [FIND-T01-1, FIND-T01-2, FIND-T01-3, FIND-T01-4]
skills: [$wyrd-implement]
---

# T01-R1 Close framework compatibility findings

## Subject and authority

- Approved specification: `changes/active/mcp-compat/spec.md`, revision 3.
- Original task: `changes/active/mcp-compat/tasks/T01-framework-journeys.md`;
  immutable original text is at `84c2af51b42c62edf2c596b5b600cadf9094b61a`.
- Original cumulative base: `84c2af51b42c62edf2c596b5b600cadf9094b61a`.
- Reviewed implementation candidate: `6fbd60f19163ea10689029982b809a158dbc4987`.
- Prior verdict: `changes/active/mcp-compat/review/T01-6fbd60f19/verdict.md`.
- Independently validated diagnoses:
  `changes/active/mcp-compat/review/T01-6fbd60f19/findings-validation.md`.
- Authority transition:
  `changes/active/mcp-compat/review/T01-6fbd60f19/spec-r3-handoff.md`.

The prior verdict remains an immutable r2 review. The owner's subsequent
direction authorizes the r3 model-alias boundary and this remediation packet.
This task implements the four retained findings under r3; it does not relabel
the old candidate PASS. A later review must inspect the original cumulative
base through the new candidate, including this task and the prior findings.

## Outcome and ownership

Prove complete discovery, valid model-facing aliases that still call Wyrd's
original tool, and real ADK handling of a server RBAC refusal. Repair the four
Rust documentation contracts in the same bounded change. Keep the existing
server behavior, gateway permission fix, tool names, framework clients,
fixtures, and dependency group.

One task is sufficient: these are corrections to T01's proof and its required
documentation. No new capability, library, adapter package, transport, or
shared harness is needed.

| Capability | Existing owner and consumers | Precise gap | Selected correction |
|---|---|---|---|
| TS complete catalog | Official `Client.listTools` in `mcp-reference-client.test.ts`; existing `serverTest` fixture; SDK `Protocol.request` and server `list_tools` | Hint categories do not prove the list contains every server-advertised name | Compare with a separate unfiltered `client.request({method: "tools/list"})` on the same administrator connection. Reuse the SDK's typed protocol request, not handwritten HTTP/SSE or a second client. |
| ADK denied-call consumer | `McpTool.run_async`, `MCPSessionManager`, existing real viewer fixture and offline ADK runner | Existing reader test stops at the runner's local unknown-name lookup | Describe the hidden call using an administrator-discovered descriptor, invoke an ADK `McpTool` bound to a viewer-authenticated manager, then read guide through that same manager. |
| Provider aliases | Native OpenAI qualification, native Pydantic renamed toolset, existing small `ProviderNamedMcpToolset`; full framework catalog tests | r2 prohibited the local ADK alias; r3 authorizes it but requires explicit scope, identity preservation and honest documentation | Retain the existing small ADK override; keep alias work at model presentation, verify valid unique catalog names, preserve original wire invocation, and document the required settings. |
| Rust panic contracts | Three gateway descriptor projections call existing `gated`; catalog unit test asserts schemas | Four changed items omit required panic sections | Add concise rustdoc to those four items only. |

The public SDK `request` overload is present in the locked official TS client
2.3.1. Its `listTools` path additionally applies tool exclusion while a generic
typed request returns the server result. Wyrd's current `list_tools` returns a
single page. The installed ADK 2.12 public `McpTool` and `MCPSessionManager`
already own invocation, error conversion, transport pooling, and closure.
These existing owners remove any justification for replacement machinery.

## Finding diagnoses and decision-complete recommendations

### FIND-T01-1 — TS full-catalog proof

**Diagnosis.** At `mcp-reference-client.test.ts:24–26`, the test reduces the
catalog to `{true, false}` effect hints. A subset containing guide and one
write satisfies it. SDK schema validation cannot prove completeness against
an unobserved source. REQ-003 and original Scenario 1 require a server-derived
name comparison. The prior green execution does not exercise that assertion.

**Selected correction.** Keep the connected administrator client and call its
public generic `request({method: "tools/list"})` for an independent unfiltered
server result, then compare complete sorted names with `listTools()` output.
Do not use `listTools()` twice as the reference: both traverse the same
filtering path. Assert that the raw result has no next cursor, matching the
current unpaginated server owner; do not build a pagination engine for a path
the server does not expose. If pagination becomes real, use the existing
SDK's facilities and revisit that assertion instead of silently comparing
one page. Preserve `listTools()` and the guide call, because they populate and
exercise the SDK's advertised-output validation. Remove the hint-set check if
the full-name assertion makes it redundant. No schema-root loop or hard-coded
tool array is allowed.

**Closure.** Name equality must fail when one ordinary client-observed tool
is temporarily omitted, even if both read/write hint categories remain.
Restore that mutation and record the passing exact reference journey.

### FIND-T01-2 — ADK actual server refusal

**Diagnosis.** At `test_agent_frameworks.py:331–346`, the viewer's tools omit
`verification.execute`. The scripted model requests `verification_execute`
anyway; ADK returns a local tool-not-found error before MCP invocation. The
test accepts any `error` key. Wyrd's denial code and ADK's handling of a real
server refusal are never observed, so a crashing MCP refusal consumer would
not fail the test.

**Selected correction.** Keep the ordinary viewer discovery comparison. For
the explicit malicious/known-name call, obtain the real hidden tool's public
descriptor from the administrator catalog, and wrap it using ADK's public
`McpTool` with an `MCPSessionManager` whose connection parameters contain only
the viewer bearer. Use the existing offline ADK runner/model machinery to
invoke that native tool and then the native guide tool using the same manager;
the runner supplies its normal tool context. Keep this deliberate explicit
call distinct from the viewer-discovered catalog. Possessing a descriptor
does not grant permission. The existing well-formed UUIDv7 arguments ensure
the failure comes from authorization rather than parsing.

Require the native framework tool-error result to contain
`WYRD_PERMISSION_403_DENIED_RBAC`, then require the subsequent guide result's
`topic` to be `overview`. Keep the manager's own close lifecycle in `finally`.
Use the already permitted ADK environment settings; do not add a separate
error converter, exception-swallowing wrapper, fake tool context, new feature
switch, raw MCP substitute, or private session-manager access. ADK's native
graceful-error handling is already enabled by default in the locked version.
Minimal model aliases on explicitly supplied native tools are permitted by
r3; they must preserve the descriptor's original wire name.

This selects ADK's existing public tool/session owners instead of extending
server authorization or building another generic-call API. No hidden write
is added to the viewer's real advertised catalog. The server's unchanged
`VerificationControl::execute` authorizes before target lookup; no real
Verifier registration or execution fixture is necessary for this denial.

**Closure.** The exact reader test must observe the Wyrd code through ADK and
a guide read afterwards. A locally manufactured `tool not found` response or
an arbitrary error dictionary must fail the assertion. The code-specific
assertion establishes server provenance without a second logging/test hook.

### FIND-T01-3 — Authorized model aliases with preserved wire identity

**Diagnosis.** The r2 review correctly found that native ADK preserves dotted
tool names and its prefix option cannot remove dots. The current copied-tool
override at `test_agent_frameworks.py:174–188` made the regex pass through
custom adaptation that r2 excluded. Public docs at `mcp-tools.md:29` described
that adaptation without approved compatibility authority.

**Resolved decision.** Approved r3 permits small local model-name aliasing
while prohibiting a new MCP transport/SDK adapter or any Wyrd wire rename.
Do not replace or generalize the current ADK override. Retain its shallow
copies with only the model-facing `name` changed; native ADK retains the
original descriptor name for its own call path. Reuse native OpenAI
`include_server_in_tool_names` and Pydantic `.renamed(...)` unchanged. ADK's
prefix-only option is not a substitute because it leaves embedded dots.

Verify complete model-facing names satisfy the regex and are unique for each
framework's actual advertised catalog. Reuse existing catalog comparisons and
model observations; add only the missing uniqueness assertion where needed.
Keep the existing aliased guide round trip as routing proof. No alias
registry, fallback name generator, hash suffix, generic plugin system,
configuration file, extra package, or production SDK export is authorized.
The finite discovered Wyrd catalog is the scope; a future real alias collision
must fail proof instead of silently selecting the wrong tool.

Update the MCP framework reference to say exactly which native setting or
small local override was tested. Clearly distinguish model aliases from
dotted MCP wire names. State that ADK needs its experimental modern-protocol
setting, that the local override supplies provider-safe names, and that its
MCP client still owns calls and errors. Keep the existing non-Google mTLS
setting and explanation. Do not describe unconfigured ADK as satisfying the
regex. The existing local override is sufficient; it is not a new shipped
Wyrd adapter product.

**Closure.** Full-catalog valid/unique model-name assertions and a guide call
through the aliased name must pass. Record the r3 authority transition in
implementation evidence; do not rewrite the old r2 review to erase the
finding or claim policy approval alone passes implementation review.

### FIND-T01-4 — Required Rust panic documentation

**Diagnosis.** `mcp/gateway.rs:323–353` introduces `descriptors`,
`descriptors_unscoped`, and `write_descriptors`. Each calls `gated`, whose
embedded-schema parse can reach the existing invariant panic. `gated`
documents it, but these entry points do not. The materially changed
`mcp/mod.rs:439–470` catalog unit test asserts presence and schema invariants
without a panic contract. AGENTS §16 explicitly applies to these items.

**Selected correction.** Add a concise `# Panics` section naming the embedded
object-schema invariant to each of the three projections; referring to
`gated` is sufficient. Document assertion failures on the existing catalog
unit test. Keep every runtime body and schema assertion unchanged. Do not
convert this into a Result-returning API, add malformed-schema fixtures,
sweep unrelated rustdoc, or create a documentation-only follow-up round.

**Closure.** Compare the four item docs to their complete bodies and `gated`,
then run normal Rust formatting. This is a static correction; no manufactured
RED or runtime test is needed for documentation alone.

## Ordered execution and proof

1. **TS completeness — Behavior:** compare full server/client name sets and
   retain structured guide validation. **RED:** temporarily omit one ordinary
   client-observed tool and show the new equality fails. **GREEN:** restore
   the list; the exact reference test passes. **REFACTOR:** remove only the
   now-redundant hint-set surrogate; reuse the existing client and fixture.
2. **ADK denied call — Behavior:** actual hidden write under viewer authority
   becomes a Wyrd-coded native tool error, followed by successful guide read.
   **RED:** add the Wyrd-code assertion to the existing local-error path and
   observe its expected failure. **GREEN:** use the selected public native
   tool/session path and pass the exact reader test. **REFACTOR:** remove the
   redundant local unknown-tool negative case rather than maintain two proofs
   for this requirement; retain ordinary reader catalog coverage.
3. **Model aliases — Behavior:** every model-facing catalog has valid unique
   names and guide still reaches its original wire tool. **RED:** where the
   behavior already passes, use verification-only evidence; if adding a
   missing assertion, temporarily duplicate one observed alias to prove that
   assertion rejects collision, then restore it. **GREEN:** existing native
   options/local ADK override pass. **REFACTOR:** keep the current small local
   mechanism and avoid any new abstraction. Specification/docs/rustdoc edits
   use static proof rather than fabricated TDD.

Run behavioral scenarios one at a time. Record expected failures and restored
passes. Do not modify shared server schemas, permission checks, timeouts,
framework dependencies, skips, or installed dependency source to manufacture
a green result.

## Acceptance criteria

| Finding | Required closure | Proof |
|---|---|---|
| FIND-T01-1 | Exact complete TS name equality against independent unfiltered server result, with SDK guide validation preserved | Focused TS journey plus one omission sensitivity experiment |
| FIND-T01-2 | ADK native call under viewer credentials receives Wyrd RBAC code, then native guide succeeds using same manager; real viewer catalog still has no writes | Focused ADK reader test; wrong/local error does not satisfy code assertion |
| FIND-T01-3 | r3 alias boundary respected; all framework model names valid/unique; alias guide call preserves original wire identity; docs describe required configuration honestly | Existing framework catalog observations/round trips and static docs/owner trace |
| FIND-T01-4 | Four cited Rust items document their actual panic conditions without behavior changes | Static rustdoc/body comparison and Rust formatting |

## Write set, invariants, and non-goals

Expected implementation changes are the existing TS reference test, existing
Python framework journey module, MCP reference documentation, and rustdoc on
the cited gateway/catalog items. Update T01/remediation execution evidence to
use r3 and actual current focused test paths. The spec revision and T01
authority reconciliation are already prepared with this packet.

Preserve INV-001/002/003 and the gateway visibility root fix. Schema-root proof
stays in Rust. Keep protocol 2026-07-28, original dotted wire names, strict
conversion enabled, test-only dependency isolation, offline providers, and
existing permission/audit/secret/session owners. Add no dependencies, Cargo
features, routes, server aliases, resources/prompts/sampling, SDK public APIs,
new host configuration, or durable behavior. Do not inline schemas merely to
eliminate OpenAI's allowed non-strict fallback.

## Focused verification commands

Use existing native setup; if missing, run the current `ts:build` /
`ts:build:testing` or `py:setup` tasks as setup, sequentially with all other
Cargo-backed work. Keep the framework group installed while examining ADK
source. The following selectors name the existing tests exactly:

```bash
# FIND-T01-1: real TS reference journey and omission-sensitivity iteration.
WYRD_LOG=info scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/mcp-reference-client.test.ts -t "^the official MCP SDK lists the full catalog and validates a structured read$"'

# FIND-T01-2: actual ADK authorization refusal and recovery.
WYRD_LOG=info scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run --group mcp-frameworks python -m pytest -q tests/integration/mcp_frameworks/test_agent_frameworks.py::test_google_adk_reader_sees_and_reaches_no_write_tool'

# FIND-T01-3: ADK complete declarations, alias uniqueness and guide routing.
WYRD_LOG=info scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run --group mcp-frameworks python -m pytest -q tests/integration/mcp_frameworks/test_agent_frameworks.py::test_google_adk_declares_every_tool'

# Narrow module lane covers the same alias contract for OpenAI/Pydantic too.
mise run py:test:mcp-frameworks

# Changed-language and changed-doc checks; use check-only formatting during proof.
mise run py:format:check
mise run py:lints
mise run ts:format:check
mise run ts:lints
mise run ts:typecheck
mise run fmt
mise run docs:check
git diff --check
```

`mise run fmt` is the normal formatting action, not a reason to retain
unrelated formatting changes. Inspect the resulting diff. Python's current
default typecheck deliberately excludes this optional-dependency directory;
do not claim it typechecks these journeys or run it as substitute evidence.
Rust changes are rustdoc-only, and no generated contract/stub is expected to
change; no new Rust test run, workspace Clippy, or codegen rerun is required
solely for those comments. If implementation goes beyond this write set's
declared behavior, apply AGENTS checks appropriate to the actual change.

The broader MCP/SDK/user-journey suites and final integrated aggregate belong
to change review once. Do not add `gate`, a full SDK integration suite, or
unrelated language tests to this remediation task.

## Completion evidence and stop conditions

Append actual commands/results, sensitivity failures, restored green results,
finding-by-finding closure locations, and excluded non-goals to this task's
execution record. Preserve the prior review reports. Re-review the original
base through the new immutable cumulative candidate; keep FIND-T01-1 through
FIND-T01-4 stable.

Stop only if the selected public dependency owners no longer provide their
observed APIs, correct aliasing requires changing wire identity or adding a
new transport/SDK adapter, or the real server refusal requires a public error
contract change. A missing local setup, stale focused selector, or routine
private test arrangement is a reversible implementation correction, not a new
approval gate. Do not treat current native ADK dotted declarations as an
automatic stop: r3 explicitly permits the bounded local alias already present.

Handoff: `$wyrd-implement` with this task. This packet does not implement,
commit, merge, push, deploy, or claim the findings closed.
