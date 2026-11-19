---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-verifier-code-mode
spec_revision: 2
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-006, REQ-007, REQ-008, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, INV-001, INV-004, INV-006, AC-001, AC-002, AC-004, AC-005, AC-011, AC-012, AC-013, AC-016, AC-017]
depends_on: [TASK-001]
parent_task:
remediates: []
---

# Use code-mode verification through every public client

## Outcome and Value

Python, TypeScript and MCP callers can declare the same Agent orchestration,
execute it and inspect honest tool-call evidence through their normal public
APIs. Public guidance explains how to use it and repair an ordinary denied
call. For example, a check can read selected evidence inside Python and return
a supported judgment; a caller sees the outer result as model-directed and
the nested evidence reads as code-directed, including a denied read.

TASK-001 supplies the server bridge, wire fields, shared Rust journey and
exhaustive security/resource/recovery proof. This task completes the client
workflow and documentation. It adds no client-owned execution or permission
behavior, and does not repeat the reduction comparison or server fault matrix.
Read this task and approved spec revision 2; the task does not rely on the
TASK-001 document or author conversation to define its contract.

## What Changes and What Must Stay True

The delivered server contract is:

- Agent-generated `code.run` can `await call_tool(name, arguments)` for exact
  dotted built-ins declared on that Agent, excluding recursive `code.run`.
  Arguments are one JSON object, results are bounded JSON converted to Python
  values, and ordinary failures are catchable `RuntimeError` containing safe
  catalog problem JSON. Authored code actions/checks acquire no bridge.
- The existing provider catalog and schemas remain. Intermediate values go
  to Python, not automatically to the model. The server owns tenant/accepted
  authority, exact tool and downstream permissions, audit and all limits.
- `ToolCallRecord.recipient` has wire values `model` and `code`; absent old
  records decode as `model`. Newly emitted records include it. The outer code
  call is model-directed and nested calls code-directed. Digests/bytes describe
  the bounded JSON actually delivered; no-value errors have zero bytes/no
  digest. Records contain bounded arguments and safe error, never raw results.
- `RequirementResult.tool_calls` defaults empty, using the same record element
  contract as step calls. It survives valid custom-data decoding and failed/
  inconclusive Agent checks. Non-Agent checks have no calls. Existing capture,
  redaction and sensitive-result access controls still govern arguments.
- Historical result bytes/digests are not rewritten. New clients read old and
  new forms, and typed custom data changes only `data`, never other fields.
  Runtime OpenAPI/JSON schemas and MCP outputs agree with the Rust contract.
- Host cancellation/deadline, Busy and exhausted invocation allowance remain
  host outcomes, never catchable successes or false failed-subject judgments.
  Startup-only defaults are 128 requests across retries and 64 KiB arguments;
  the existing result default remains 64 KiB. No Card override/mode switch.

Project fields using the existing Python result mappings/authored stubs,
TypeScript generated wire types/thin wrappers and MCP/HTTP result schemas.
Change native binding registration/exports only where the current projection
requires it; do not create a new Python class or Node facade just for a field.
Repair any mechanical native consumers left by the additive contract, keeping
Rust/TypeScript SDKs free of Python features.

Use one checked-in declaration/story across languages. Each focused public
journey authors it, executes a nested call successfully, exercises a permission
denial, then inspects step/check records through normal immediate and retained
result APIs. Include custom Verdict data to prove typing preserves call
records. Use a real server, Postgres and Monty worker, with only the model
provider scripted locally. Success, denial and readback may share one server.
The shared Rust suite owns the full concurrency, fault, historical JSON and
reduction matrices; do not reproduce them per SDK.

Document worker occupancy during host waits, per-child authorization/audit
cost, bounded arguments/results/calls, ordinary versus host failures, output
recipients, supported pinned citations and the limits of byte/token savings.
Bridge-dependent programs require upgraded workers; existing manifest rules
govern overlap/downgrade. No old-worker equivalence or universal speed/token
claim. No new public endpoint, client execution engine, UI workflow, extraction
format, raw network/shell access or intermediate store.

## Approach

1. Revalidate the delivered wire fields/defaults against the spec and inspect
   the current SDK result conversions; extend authored/generated projections
   from their owners without duplicating validation.
2. Extend existing verification story fixtures/provider scripts with the same
   nested declaration and focused success/denial/readback phases. Keep the
   actual interpreter, authorization, storage and transport in the path.
3. Prove Python and TypeScript callers retain recipient-tagged step/check
   records, safe denial codes and custom data through their public APIs.
4. Extend the existing real MCP verification journey for the same observable
   contract and verify served HTTP/OpenAPI result schemas agree.
5. Update existing agent-tool, runtime-limit and verification usage guidance
   from the tested declaration, then run focused SDK/schema/docs checks.

## Ordered Implementation Scenarios

### Scenario 1 — A builder runs and diagnoses the same Verifier through their public client

**Behavior.** A builder authors the shared declaration, executes nested Agent
preparation/check calls, diagnoses a denied read, then reruns with the required
permission and inspects the result. Python, TypeScript and MCP expose the same
server-owned behavior, supported original-evidence citations, custom Verdict
data and honest tool records through their actual public APIs.

| Journey phase | Expected observation on each shipped surface |
| --- | --- |
| Discover and declare | Use the existing verification workflow and shared checked-in declaration. Existing provider catalog/contracts remain; MCP read tools remain discoverable and write tools scoped. No new code-mode endpoint or second schema catalog. |
| Execute successfully | A nested preparation/check call completes with the bounded outer output, `model` outer recipient and `code` children. Supported citations still point to original pinned evidence. |
| Diagnose and recover | A principal missing one downstream permission receives no protected value and the existing exact catalog permission code. The no-value call has zero bytes/no digest and its denial record remains. A correctly authorized rerun succeeds. |
| Inspect immediate and retained results | Immediate result, submitted execution wait/status and retained output preserve the same step/check records and custom Verdict data. Typed decoding changes only `data`, never drops records, and wrappers accept historical absent fields through the shared Rust decoder without new required client input. |
| Inspect the contract | MCP result shapes and served HTTP/OpenAPI components agree with Rust's additive recipient/default-empty call fields and historical optional/default behavior; authored stubs and generated types match. |

**RED.** Work through this user journey in small TDD increments on one surface
at a time, using [Python journey proof](#python-journey-proof),
[TypeScript journey proof](#typescript-journey-proof) and
[MCP journey proof](#mcp-journey-proof). Start with the smallest missing
success/readback assertion, then add denial/recovery and retained/custom-data
observations while earlier phases stay green. Each surface must select its
actual focused test and demonstrate its missing executable observation; a
single scenario does not mean one combined cross-language test or a batch of
all REDs before implementation. Use [served schema proof](#served-schema-proof)
for the served-field/default contract. If generic serialization/projection
already preserves a field, record its passing regression evidence and use the
missing journey observation for RED rather than force a production change.
Compile-only type assertions and static declarations use
[projection checks](#projection-checks), not a manufactured runtime RED.

**GREEN.** Complete only missing public mappings, authored stubs, generated
wire types and native consumers, then extend existing journey/provider support
for the same story names and declaration. Exercise the actual interpreter,
permissions, storage and transport with public client/principal setup and
typed assertions. Keep MCP on ordinary execute/enqueue/result tools, typed
outputs and scoped permissions. Rerun earlier phases and affected surface
journeys after changing shared contract, fixture or support code; do not
repeat the shared server fault or 20-file reduction matrices in each client.

**REFACTOR.** Reuse existing result serialization, typed custom-data
transformation, story fixtures and MCP result reader. Remove duplicate
field-by-field conversions or hand-authored schema copies; add no client
execution/authorization engine, code-mode facade or bridge-specific transport.
Public guidance derives from the tested workflow, with the existing static/
example proof in Verification and Evidence rather than another scenario.

## Acceptance Criteria

| Spec obligations | Owner and proof | Required observation / violation |
| --- | --- | --- |
| REQ-001–004, REQ-014; AC-001–002, AC-016 | Focused Python/TypeScript/MCP journeys | The same declared nested program works through real APIs; direct catalog remains; public callers receive only the bounded outer output. Shared Rust coverage owns the measured reduction and dependency matrix. |
| REQ-006–008; INV-004; AC-004–005, AC-017 | Success/denial/recovery phases in those journeys | Missing permission yields no protected data and exact existing denial code; the correctly authorized invocation succeeds. No client-side grant or tenant substitution. Exhaustive grant/delegation/audit cases stay in TASK-001. |
| REQ-015–017; INV-001, INV-006; AC-012–013 | SDK mapping/type checks and real immediate/queued readback | New records survive all exposed conversions with honest recipients and custom data. Historical forms are accepted; no field becomes a new required client input. Capture permissions remain server-owned and no raw intermediate value is added. |
| REQ-018; AC-001, AC-011, AC-016 | Focused supported-citation journey assertions | The check cites original pinned evidence; SDKs/MCP do not promote Python scratch/live reads to pinned evidence. Invalid citation matrix remains shared server proof. |
| AC-013; existing compatibility contract | Generated/stub checks and served OpenAPI/MCP proof | Wire/source/stub declarations agree and the new budget catalog code projects from Rust. No checked-in JSON Schema, handwritten generated field or rewritten historical result. |
| REQ-001–004, REQ-008, REQ-014–018; AC-002, AC-012–013 | Existing docs updated against tested declarations and docs check | A reader can declare/invoke/inspect/diagnose the bridge with correct authority, limits, recipients, citations and upgrade requirements; no universal token/latency guarantee. |

## Implementation Reference

Inspected existing owners and precise gaps; paths are guidance rather than
an implementation allowlist:

- `sdks/wyrd-sdk-python/src/verify.rs::PyVerificationResult::{requirements,
  steps, typed}` serializes Rust wire objects using `json_to_pyobject` and
  transforms custom data through `decode_data`. Runtime generic serialization
  may already preserve new fields; validate it rather than replace it.
  `python/wyrd/verify/__init__.pyi::{RequirementResult, ToolCallRecord}` has
  authored TypedDict declarations lacking the new fields. Preserve public
  exports/native registration rules; add exports only if required by an
  existing public type. Public integration tests import `wyrd.verify`.
- `sdks/wyrd-sdk-ts/wyrd/src/card-types.ts` is generated wire output;
  `src/index.ts::{RequirementResult, typedResult}` derives requirements from
  that wire contract and spreads all non-data fields. Extend the generating
  owner and verify typed preservation rather than duplicate a result type.
  `native/src/verification.rs` is the existing thin input/handle projection.
  `tests/types/verification-data.test-d.ts` contains compile-time verification
  data assertions; extend these for optional historical and new record fields.
- `crates/shared/wyrd-client/src/verify/execution.rs::Execution` and Rust SDK
  `src/lib.rs` already project shared execution contracts. Keep canonical Rust
  decoding/default behavior; the server task owns the new wire contract.
- Existing story tests are `sdks/wyrd-sdk-{rust,python}/tests/integration/`
  `verifier_building_blocks.rs` / `test_verifier_building_blocks.py` and
  `sdks/wyrd-sdk-ts/wyrd/tests/integration/verifier-building-blocks.test.ts`.
  They already have policy/service setup, scripted providers, tool/domain
  denial, custom-data and retained readback flows. Extend those facilities
  rather than invent a parallel SDK test harness. Shared Cards live in
  `fixtures/cards/verifier_building_blocks/`, including the investigator,
  tool-review and scored-review declarations; add only the smallest nested
  declaration/evidence needed without rewriting existing regressions.
- `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs::pg_tests` already
  drives a real `rmcp` client over execute/enqueue/get-execution and Bifrost
  readback. Its `judging_server` configures a local provider and real runtime.
  Reuse those workflows for nested step/check records. This module is in
  package `wyrd-mcp`, explicit target `mcp`, not the server integration binary.
- `crates/wyrd/wyrd-server/src/mcp/verification.rs` projects typed verification
  handlers; `tests/integration/pg_openapi_contract.rs` owns served-schema proof.
  Extend the current OpenAPI test module; add no committed schema artifact.
- `docs/src/content/docs/reference/agent-tools-and-authorization.md` currently
  describes step-only records and workspace-only code; update both facts.
  `reference/verification-runtime-limits.md` and
  `operate/configure-verification-runtime.md` own host-setting meaning and
  startup guidance. Update the existing verification author/run-and-inspect
  guide where the tested workflow belongs. Follow `docs/AGENTS.md`,
  `docs/CONTRIBUTING.md` and `docs/PRODUCT.md`; copyable code comes from a tested
  example via `CodeFromFile`, never a separately authored untested snippet.
- Governing architecture/security corrections belong to TASK-001. Check their
  delivered descriptions agree with public guidance rather than re-deciding
  the portable boundary or accepted authority here.

## Verification and Evidence

Run from the repository root. All **planned** names below are new focused
extensions in the inspected existing owners. Confirm the actual selector after
adding each test and before its RED run, recording any equivalent correction.
Tests must actually be selected; a filtered zero-test success is not proof.
Cargo-backed builds/tests/codegen/lints run sequentially across the checkout.
Set `WYRD_LOG=info,wyrd_server=debug,vala_verify=debug` to diagnose failures.

### Provisioning

Run `mise run build:server`, `mise run py:setup`, `mise run ts:build`, and
`mise run ts:build:testing` sequentially before focused SDK/MCP tests as needed.
They provision the same real-worker and dev bindings used by the owning lanes.
The Postgres wrapper below supplies repository-local databases/variables;
never infer a worker executable from a target-directory layout or require
paid-provider credentials.

### Python journey proof

**Planned** top-level function in the current integration story, public
`wyrd` imports and existing server/provider fixtures:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- sh -c 'cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_verifier_building_blocks.py::test_code_mode_reduces_evidence_and_preserves_citations'
```

The test name matches the shared Rust/TypeScript story, but its focused proof
is success, denial, custom-data and record readback; the 20-file comparison
runs only in shared Rust coverage.

### TypeScript journey proof

**Planned** test with exact title `code_mode_reduces_evidence_and_preserves_citations`
in the current story and existing `serverTest` real-server/provider setup:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- sh -c 'cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/verifier-building-blocks.test.ts -t "^code_mode_reduces_evidence_and_preserves_citations$"'
```

### MCP journey proof

**Planned** real-client test in the existing `pg_tests` module. Use the actual
MCP execute/enqueue/result tools, not direct calls to their handler functions:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=verification::pg_tests::code_mode_preserves_authority_and_records)'
```

### Served schema proof

**Planned** test in the current server OpenAPI integration module; prove the
served result component fields and default/optional semantics, not only a
locally generated schema:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_openapi_contract::verification_tool_records_match_served_contract)'
```

### Projection checks

For authored stubs, exports and SDK types run `mise run py:format:check`,
`mise run py:lints`, `mise run py:typecheck`, `mise run ts:format:check`,
`mise run ts:lints`, `mise run ts:typecheck`, `mise run ts:napi:check`, and
`mise run ts:pack:check`. The native declaration check and typecheck own
binding rebuilds; run them sequentially. Extend existing compile-only result
type assertions for recipient values, default-empty calls and custom-data
preservation; no manufactured runtime RED for static declarations.

Use `mise run codegen:regen` only when source changes require new generated
outputs, then `mise run codegen:check`. Run `mise run docs:check` for the
affected public docs/tested examples, `mise run fmt:check`, `mise run lints`,
and `mise exec -- git diff --check` for changed native Rust and packet closure.
Required checks must pass under the repository's execution rules. A generated
checker reporting an intentional diff is still a red result: diagnose it at
its owner and record any genuinely equivalent correction under the execution
authority. Do not bypass the check, declare a red gate passed, hand-edit a
generated artifact, or commit merely to satisfy a HEAD comparison. If resolving
it requires caller-owned Git authorization, report that exact remaining gate
rather than claiming task completion.

Record focused behavioral RED/GREEN results or justified already-correct
projection regression evidence, runtime/type/stub/codegen/schema results,
actual public denial codes and recipient-tagged immediate/retained examples.
No full SDK journey suites or server fault matrices belong to this task.
At integrated change review run the existing affected Rust/Python/TypeScript/
MCP verification journey lanes and contract/docs checks once, with the spec's
coverage grouping; no duplicate reduction comparison or universal token claim.

## Material Stop Conditions

Stop for planning/spec authority if a public consumer cannot preserve the
approved fields/defaults without changing a wire contract, new durable client
behavior, widened permissions, a new endpoint, dependency/feature, migration,
or weaker public journey proof. Private wrapper/support/fixture structure,
mechanical exports/stubs/generated consumer repair and equivalent selectors
remain implementation decisions. A missing runtime projection already
covered by the approved fields is in scope; do not treat routine wiring as a
new approval requirement.

## Authority Links

- [Approved revision 2](../spec.md), especially public compatibility,
  authority reconciliation and evidence grouping.
- [Repository rules](../../../../AGENTS.md), [execution discipline](../../../implementation-execution.md),
  [testing](../../../../TESTING.md), [agent surfaces](../../../../agent_docs/agent-surfaces.md),
  [errors](../../../../agent_docs/errors.md).
- [Python SDK rules](../../../../sdks/wyrd-sdk-python/AGENTS.md),
  [TypeScript SDK rules](../../../../sdks/wyrd-sdk-ts/AGENTS.md),
  [docs rules](../../../../docs/AGENTS.md), [docs contribution](../../../../docs/CONTRIBUTING.md),
  [product vocabulary](../../../../docs/PRODUCT.md).
- [Verifier authority](../../../../architecture/wyrd-design.md#verifier),
  [security posture](../../../../architecture/wyrd-security-posture.md),
  [rollout compatibility](../../../../architecture/operations/deployment-and-release.md#rollout-and-version-skew).

## Implementation Evidence

Status: `IMPLEMENTED`. Commits `37ca6ff89`, `757e7d29e`, `fbab11515`,
`d90ebcea6`, `907575b0f`, `651c6009b`.

Capability revalidation: TASK-001 already projected `recipient` and
requirement `tool_calls` through every runtime path, including Python's
generic `json_to_pyobject` result mapping, TypeScript's generated
`card-types.ts` and its `typedResult` spread, the MCP output schemas and
served OpenAPI. The gaps were the authored Python stub (`RequirementResult.tool_calls`,
`ToolCallRecord.recipient` and `ToolRecipient`), the static type fixtures, the
public journeys and the docs. No runtime binding, export, endpoint or
dependency was added. The journeys therefore pin existing behavior: their
GREEN result is regression proof, not a RED-then-GREEN fix. RED evidence for the new assertions:
narrowing the TS `recipient` assertion to `"model"` alone produced one
`ts:typecheck` error, and mutating the expected OpenAPI `recipient` enum
failed `verification_tool_records_match_served_contract`.

One checked-in story is shared by every surface. `fixtures/cards/verifier_building_blocks/programs/nested-investigation.py`
reads a Card, then queries Bifrost, catches the denial by catalog code and
repairs it. `reduce-notes.py` lists, reads and searches the notes inside Python
and prints only the hits. The Rust SDK and MCP tests load both programs with
`include_str!`, Python with `read_text`, TypeScript with `readFileSync`, and
the docs embed the first program with `CodeFromFile`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001–004, REQ-014; AC-001–002, AC-016: the nested program runs through the public APIs and only bounded output reaches the model | Python `test_code_mode_reduces_evidence_and_preserves_citations`, TS `code_mode_reduces_evidence_and_preserves_citations`, MCP `code_mode_preserves_authority_and_records` | Each asserts that no note text reaches the model turn, that the outer `code.run` record is `model` and the children are `code`, and that the direct fs.* catalog is still offered | PASS |
| REQ-006–008; INV-004; AC-004–005, AC-017: a denial yields no data, and an authorized retry succeeds | The NESTED phase under a key or role without `bifrost_query:read`, then with it | Denied `bifrost.query` record has `recipient` `code`, `error` `WYRD_PERMISSION_403_DENIED_RBAC…`, no `output_sha256` and `returned_bytes` 0. Printed `query WYRD_PERMISSION_403_DENIED_RBAC repaired True`, then `query allowed repaired False` | PASS |
| REQ-015–017; INV-001, INV-006; AC-012–013: records and custom data survive every read path | `assert_reduced`/`assertReduced` over verify, verify_typed, submit→wait/wait_typed/status and retained `bifrost.verification.results`; MCP execute, enqueue/settle and Bifrost readback | Same journeys, with typed `Hits` custom data; `py:typecheck` `verify_contracts.py` and `verification-data.test-d.ts` assert the optional `tool_calls` and the `"model" \| "code"` recipient | PASS |
| REQ-018; AC-001, AC-011, AC-016: citations stay on the original pinned evidence | Journey assertions that the check cites `/inputs` notes (`NOTES_CITED`) | Same journeys | PASS |
| AC-013: wire, stub and generated declarations agree; the budget code projects from Rust | `pg_openapi_contract.rs::verification_tool_records_match_served_contract`; authored `.pyi`; generated TS `error-codes.ts` carries `WYRD_VERIFICATION_422_TOOL_BUDGET_EXCEEDED` | OpenAPI test; `codegen:check`; `ts:napi:check`; `py:lints` stub check | PASS |
| Docs: declare, invoke, inspect and diagnose the bridge | `reference/agent-tools-and-authorization.md` (Tools from Python, step/check records with `recipient`, new error rows), `reference/verification-runtime-limits.md` (two settings, tool waits count, budget row), `operate/configure-verification-runtime.md` (worker occupancy, per-call authorization/audit, upgrade first), run-and-inspect guide table | `mise run docs:check` (generate, build, links, a11y) | PASS |

Commands (all passed):

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verifier_building_blocks::code_mode_reduces_evidence_and_preserves_citations) | test(=verifier_building_blocks::hosted_tools_need_the_tool_and_domain_grants)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=verification::pg_tests::code_mode_preserves_authority_and_records)'"
scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration --run-ignored=all -E 'test(=pg_openapi_contract::verification_tool_records_match_served_contract)'
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q tests/integration/test_verifier_building_blocks.py -k 'code_mode_reduces or hosted_tools or custom_verdict'"   # 5 passed
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/verifier-building-blocks.test.ts -t 'code_mode_reduces'"
mise run py:format:check && mise run py:lints && mise run py:typecheck
mise run ts:format:check && mise run ts:lints && mise run ts:typecheck && mise run ts:napi:check && mise run ts:pack:check
mise run fmt:check && mise run lints && mise run codegen:check && mise run docs:check
git diff --check
```

Non-goals stayed excluded: no new endpoint, client execution engine, UI workflow,
extraction format, network/shell access, intermediate store, Python class or
Node facade, and no universal token or latency claim (the docs state that
fewer turns do not guarantee savings). The shared Rust suite keeps the
concurrency, fault, historical-JSON and reduction matrices. No unrelated file
changed. The uncommitted spec revision, TASK-001 review folders,
`spec-revisions/` and TASK-003 belong to other work and were left untouched.
