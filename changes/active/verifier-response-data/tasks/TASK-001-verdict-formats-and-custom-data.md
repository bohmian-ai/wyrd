---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-verifier-response-data
spec_revision: 1
requirements: [REQ-001, REQ-002, REQ-007, REQ-011, REQ-012, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008]
depends_on: []
parent_task:
remediates: []
---

# Reusable Verdict formats with optional typed data

## Outcome and value

Add a reusable Verdict response format and optional custom data to terminal
code and Agent checks. Generate the standard schema, validate returned data,
keep it in verification results, and let Python, TypeScript, and Rust callers
receive their own typed data from verification calls.

Before: authors copy the complete Verdict schema, extra fields are rejected,
and SDK results cannot contain caller-typed custom data. After: an author uses
`response_format: verdict`, optionally adds a schema under `data`, and reads
that data from ordinary results or the approved typed operation siblings.

This one task delivers the complete [approved specification](../spec.md),
including consumer, documentation, and test closure. The spec defines the exact
public forms, limits, field placement, and failure behavior.

## Approach

Build the feature through the five TDD scenarios below, in dependency order:

1. **Declare and generate the format.** Extend the response/check contracts and
   existing Prompt compiler so named formats produce the required schema.
2. **Validate code and Agent output.** Extend both existing check executors to
   enforce the complete schema and custom-data bounds.
3. **Keep data through result readback.** Carry accepted data through engine
   reporting, the existing outcome payload, and ordinary SDK/HTTP/MCP results.
4. **Return caller types.** Add generic projections and typed SDK calls over
   existing verify/wait/status workflows; decoding must not rerun verification.
5. **Close authoring and access paths.** Exercise model/schema authoring,
   permission boundaries, and public exports; update generated contracts and
   journey-backed documentation after the runtime behavior passes.

The GREEN sections name the production changes and affected consumers. Reuse
existing owners; private helpers and fixture structure remain implementation-
owned. Keep the spec's existing judgment, citation, tenant, privacy, lifecycle,
and capture rules intact. No new compiler, result service, transport, polling
loop, configuration, or mandatory model library is part of this task.

## Ordered implementation scenarios

All new test names in [verification](#verification-and-evidence) are planned.
Confirm their actual selectors after adding them and before RED. Execute each
scenario RED → GREEN → REFACTOR; a setup failure does not establish RED.

### Scenario 1 — Generate a Verdict schema from its declared format

**Behavior.** Plain scalar/object forms generate the required response without
copied schema or prompt prose. Expanded forms compose a required custom object.
Portable/native and inline/referenced authoring agree, authored content stays
unchanged, and invalid declarations are refused before persistence.

**RED.** Add the planned declaration and compiler tests below. Observe that the
current response-format type cannot decode the named forms and validation
rejects custom fields. Assert generated request schemas, preserved authored
forms/hashes, native conflict refusals, compatible handwritten schemas, and the
check-only code declaration. Extend the public readback journey with a plain
named-format case; the declaration HTTP journey must prove refused Cards do not
exist afterward.

**GREEN.** Extend the response-format contract and `CodeCall`/check declaration
handling in `wyrd-spec` so preparation code actions retain their existing shape.
Update `VerifierValidator::validate_agent` and its canonical `verdict_schema`
comparison to accept the declared extension. Extend `AuthoredPrompt::compile`
and its existing portable/native lowering to resolve the Wyrd-owned named
schema, preserving authored content and rejecting duplicate native settings.
Do not reverse the `wyrd-spec` → `skald-spec` dependency or copy Verdict into
Skald. Update existing contract/compiler consumers so plain and handwritten
formats still compile.

**REFACTOR.** Remove repeated schema construction in this workflow where the
canonical contract now supplies it. Keep all declaration/compiler cases green;
preparation Agents may still use an explicitly declared Verdict format.

### Scenario 2 — Accept valid custom output and reject malformed or excessive data

**Behavior.** Both check kinds enforce the full declared schema, inclusive
65,536-byte/16-container limits, and the existing citation rules. Invalid output
is inconclusive, without truncation, retained rejected data, or payload leaks.

**RED.** Add the planned boundary contract and Agent execution tests, then the
invalid-output SDK journey. Exercise code and Agent missing/wrong/null/
undeclared data, unknown envelope fields, exact/over-limit values, Unicode,
JSON escapes, and wide arrays. Current Verdict decoding rejects all extras;
Agent check execution also lacks complete declared-schema validation.

**GREEN.** Extend canonical `Verdict` to carry declared data and place the pure
bounds with the response contract. Update `AgentExecutor` check execution to
validate the complete schema as its action path already does through `conforms`.
Update `CodeExecutor::execute` in the hosted sandbox to enforce the same response
contract after its existing bounded worker returns. Route invalid output through
the existing terminal-check failure path, omit rejected data, and preserve
citation filtering and the Agent pass downgrade. Rerun scenario 1 and the new
boundary/execution tests.

**REFACTOR.** Share response rules between the check owners without duplicating
their runtimes or moving code execution out of the server. Keep failures scrubbed
and existing provider/sandbox response bounds effective.

### Scenario 3 — Preserve accepted data through immediate and queued results

**Behavior.** Whole-Verifier and step requirements expose optional `data`.
Direct, queued wait/status, HTTP/MCP, and retained/published Bifrost results
agree; plain and old results omit it. Existing access boundaries remain in force.

**RED.** Add the planned SDK readback and HTTP/MCP journeys. Their local provider
and code fixtures return valid declared data; assert it after execution and
publication, rather than constructing a result in the test. Extend scoped and
foreign-tenant cases to check that the new payload grants no access. The current
result contract/reporting has no field to return or retain.

**GREEN.** Add optional `RequirementResult.data`, then update
`Judged::checked`/`report` to carry accepted data rather than discarding it.
Let `ResultPayloadBuilder::summary` persist it through the existing outcome
Variant; update ordinary result/status projections, SDK wrappers, authored
Python stubs, and source-generated TS contracts to expose the same field.
Retain existing result routes, privacy/authorization, `actual`, drift `detail`,
and durability/visibility semantics. Rerun scenarios 1–2 and readback journeys.

**REFACTOR.** Reuse full-outcome serialization and existing result projections.
Do not add a table column, new result API, or separate result lifecycle. Existing
correct access controls retain regression proof without manufactured failures.

### Scenario 4 — Return typed custom data from SDK operations

**Behavior.** The approved typed verify/wait/status calls, including Rust's
absolute-deadline wait, return actual caller-typed data or the exact local SDK
decoding error. Missing data stays absent, failed-check data is decoded, and
mixed shapes work with an appropriate union/decoder. No mismatch mutates a
result, produces a partial typed return, or repeats execution.

**RED.** Add the planned typed-success and decoder-mismatch journeys in each SDK
and extend compile-time result fixtures. Current results and operations are
nongeneric. Assert model/struct values, safe requirement/path errors, ordinary
readback after a mismatch, unchanged invocation counts, empty/in-progress status,
and a Python input slot named `data_type`.

**GREEN.** Extend the shared result projections and Rust `Observe`/`Execution`
with the approved generic and typed operations over their existing complete
execution/status/wait workflows. Project them through Rust exports, Python
observe/verify bindings and stubs, and TS `Observe`/`verify.Execution`. Use
Python `model_validate`, TS `parse`, and Rust Serde to convert all present data
before returning. Add the cataloged SDK decoding error and map failures at the
normal boundaries. Keep transport, evidence upload, polling, deadlines, and
server judgments unchanged. Rerun ordinary readback and typed success/failure.

**REFACTOR.** Reuse existing model-conversion patterns without making Pydantic
or Zod mandatory, storing model identities, or duplicating polling/transport.

### Scenario 5 — Author custom schemas with supported models and publish the contract

**Behavior.** Python/TS users can supply supported schema/model objects for the
custom schema without writing it twice. Public imports, runtime schemas, SDK
types, examples, and docs describe the same implemented feature.

**RED.** Extend the planned typed-return journey's authoring assertions and the
existing schema-coercion/type fixtures. Current response-format helpers cannot
express the named Verdict-plus-data form. Assert the authored custom schema and
its typed result through public APIs.

**GREEN.** Extend the existing Python Prompt coercion and TS `jsonSchemaText`
authoring paths to accept the approved custom format with supported models.
Update public exports, Python's authored stubs, and generated contract/error/
native declarations from their owners. Rerun schema authoring, typed-result,
and ordinary-call regressions. Update owning Prompt/Verifier architecture,
policy-judge examples, model-judge recipe, result-nesting docs, and limit
reference. Prose and generated artifacts use static proof, not a fabricated RED.

**REFACTOR.** Reuse schema conversion and examples already exercised by the
journeys. Remove repeated handwritten Verdict schemas where the new format
replaces them, keeping the compatible handwritten path covered.

## Acceptance criteria

| Scenarios | Expected evidence | Spec coverage |
| --- | --- | --- |
| 1 | Named/plain/custom forms compile consistently; invalid code/Agent declarations persist no Card; authored content and supported handwritten forms survive. | REQ-001, REQ-002; AC-001, AC-003 |
| 2 | Complete validation and inclusive bounds work for both kinds; invalid output is inconclusive with no rejected data, leakage, or changed citation policy. | REQ-007; AC-004, AC-005 |
| 3 | Accepted data survives every result path and step requirements; scoped/foreign readers gain no access; standard fields are unchanged. | REQ-002, REQ-011; AC-002, AC-007 |
| 4 | Typed operations produce actual types or the exact local error, without synthesis, partial returns, mutations, or reruns. | REQ-012; AC-006, AC-007 |
| 5 | Public model/schema authoring, types, generated surfaces, and current authority/docs/examples agree. | REQ-001, REQ-002, REQ-012; AC-008 |

## Existing owners and precise gaps

The source was inspected while creating this packet. Revalidate the relevant
owner before changing it; these paths guide reuse, not a private file allowlist.

- **Response and declaration:** `Verdict`, `RequirementResult`,
  `VerificationOutcome`, and `VerificationExecutionState` in
  `crates/wyrd-spec/src/verification/execution.rs` are fixed/closed and have no
  custom-data channel. `VerifierValidator::validate_agent` and `verdict_schema`
  in `card/verifier/validate.rs` already fit authored schemas to Verdict; extend
  that owner. `CodeCall` in `card/verifier/mod.rs` is shared by checks/actions:
  introduce the approved check-only authoring without extending code actions.
- **Prompt compilation:** `AuthoredPrompt::compile`, `native_request`, and
  `portable_request` in `crates/skald/skald-spec/src/authoring.rs` own authored
  forms and provider lowering. Extend that workflow with Wyrd-owned named schema
  resolution; `wyrd-spec` already depends on `skald-spec`, so a reverse
  dependency or a copied Verdict definition is inappropriate. Existing schema
  and native round-trip tests are regression owners. Python Prompt coercion in
  `skald-prompt/src/coerce.rs` and TS `jsonSchemaText` already accept supported
  model/schema objects; extend those authoring paths rather than a new DSL.
- **Checks and reporting:** `AgentExecutor` check execution in
  `crates/vala/vala-verify/src/blocks/agent.rs` decodes Verdict and validates
  citations, but lacks the full authored-schema validation already used by its
  action path's `conforms`. `CodeExecutor::execute` in
  `wyrd-server/src/verification/hosted/sandbox.rs` runs the bounded code worker
  and decodes Verdict. Extend both paths and `Judged::checked`/`report` in
  `vala-verify/src/blocks/engine.rs` to preserve accepted data. Reuse Agent
  integration tests in `vala-verify/tests/integration/blocks_agent.rs`.
- **Storage and result reads:** `ResultPayloadBuilder::summary` in
  `wyrd-server/src/verification/results.rs` serializes the full outcome;
  `ResultsTable` in `vala-bifrost-redux/src/tables/verification/results.rs`
  already provides sensitive Variant storage. Existing execution HTTP routes
  and MCP tools expose it. Extend the payload rather than the table or lifecycle.
- **Client and SDK projections:** `Observe::verify` in
  `wyrd-client/src/observe/mod.rs` owns binding/evidence/execution; `Execution`
  in `wyrd-client/src/verify/execution.rs` owns status and bounded polling.
  Typed operations reuse these complete workflows. Rust SDK `src/lib.rs`
  re-exports fixed types. Python `src/observe/mod.rs`, `src/verify.rs`, and
  authored observe/verify stubs expose native results with typed dictionaries;
  Bifrost `_client.py` already uses caller `model_validate`. TS `Observe` and
  `verify.Execution` in `wyrd/src/index.ts` wrap the Rust handles, with generated
  `card-types.ts`. Local conversion must not duplicate transport or polling.
- **Public proof owners:** Rust/Python/TS `verifier_building_blocks` stories
  already use checked-in `fixtures/cards/verifier_building_blocks`, local
  provider fixtures, hosted code, and direct/queued verification. Reuse that
  deployment setup. Python type coverage lives in
  `tests/typecheck/verify_contracts.py`; TS compile-only tests in
  `wyrd/tests/types/*.test-d.ts`. HTTP proof belongs to
  `wyrd-server/tests/integration/pg_verification_routes.rs`; MCP proof to
  `wyrd-mcp/tests/bifrost/mcp/verification.rs::pg_tests`.

## Verification and evidence

The new tests named below are planned, not already present. Their package,
target, runner, and setup are confirmed by the existing owners. After adding
each test, confirm and record its actual exact selector before RED. A private
placement/name correction is implementation-owned, but its command must select
a real test. SDK stories use checked-in Cards and public APIs; pure boundary
calculations support rather than replace execution journeys.

Confirmed runner contexts and setup:

| Owner | Runner context | Setup |
| --- | --- | --- |
| Contracts/compiler | `wyrd-spec --lib`; `wyrd-skald-spec --lib` | `mise exec -- cargo nextest`; no services. |
| Agent integration | `vala-verify --test integration`, `blocks_agent` module | Same nextest runner; local in-process providers, no Postgres. |
| Rust SDK story | `wyrd-ai --test integration`, `verifier_building_blocks` module | `-P journey --run-ignored=all`; existing dev-dependency features. |
| HTTP / MCP | `wyrd-server --features test-support --test integration`, `pg_verification_routes`; `wyrd-mcp --test mcp`, `verification::pg_tests` | MCP uses journey profile and ignored-test inclusion; existing server test-support. |
| Python story | `sdks/wyrd-sdk-python/tests/integration/test_verifier_building_blocks.py` | `mise run py:setup`; project `uv run python -m pytest -q -m integration`. |
| TypeScript story | `sdks/wyrd-sdk-ts/wyrd/tests/integration/verifier-building-blocks.test.ts` | `mise run ts:build`, `mise run ts:build:testing`; workspace `pnpm exec vitest run`. |

Server journeys first run `mise run build:server` for the hosted worker.
Install Python and TS prerequisites from the table only as needed, using
`mise run ts:install` if the TS workspace is not installed. The commands below
use this shell convenience over the existing isolated Postgres wrapper; it
creates no script or new lifecycle owner:

```bash
with_verification_postgres() {
  mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc \
    'mise run db:migrate:all:inner && exec "$@"' verification-tests "$@"
}
```

### Contract and execution tests

All four names below are planned. Run each exact command during its scenario:

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::verifier::validate::tests::named_verdict_formats_validate)'
mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib -E 'test(=authoring::tests::named_verdict_formats_preserve_authored_forms)'
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=verification::execution::tests::custom_verdict_data_obeys_limits)'
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::check_agent_validates_custom_data)'
```

### Focused SDK journeys

Four new tests are planned in each existing building-block story: readback,
invalid output, typed success, and decoder mismatch. Use the same names across
languages. Their case tables cover both check kinds and the relevant step,
direct/queued, empty/failed/mixed-data, and boundary variants from the scenarios;
keep each test's assertion about its named outcome. Do not build or edit Card
YAML in SDK test bodies; extend checked-in fixtures and minimal existing test
support as needed. These commands select only the task's tests, not full SDK
journey suites.

The readback test's case table explicitly includes scalar `verdict` and
`{kind: verdict}` without custom data, as well as the expanded form. The plain
Agent cases have no handwritten schema or response-format prose: they must
execute and expose the normal outcome/reason/citations with no `data`. This is
the plain-format public journey for scenarios 1–2, repeated on the three SDKs.

Rust, in `sdks/wyrd-sdk-rust/tests/integration/verifier_building_blocks.rs`:

```bash
with_verification_postgres cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verifier_building_blocks::custom_verdict_data_survives_readback)'
with_verification_postgres cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verifier_building_blocks::invalid_custom_verdict_data_is_inconclusive)'
with_verification_postgres cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verifier_building_blocks::custom_verdict_data_is_typed_on_return)'
with_verification_postgres cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verifier_building_blocks::typed_verdict_data_mismatch_keeps_original_result)'
```

Python, from the repository root (the project supplies pytest and its fixtures):

```bash
with_verification_postgres uv run --project sdks/wyrd-sdk-python python -m pytest -q -m integration sdks/wyrd-sdk-python/tests/integration/test_verifier_building_blocks.py::test_custom_verdict_data_survives_readback
with_verification_postgres uv run --project sdks/wyrd-sdk-python python -m pytest -q -m integration sdks/wyrd-sdk-python/tests/integration/test_verifier_building_blocks.py::test_invalid_custom_verdict_data_is_inconclusive
with_verification_postgres uv run --project sdks/wyrd-sdk-python python -m pytest -q -m integration sdks/wyrd-sdk-python/tests/integration/test_verifier_building_blocks.py::test_custom_verdict_data_is_typed_on_return
with_verification_postgres uv run --project sdks/wyrd-sdk-python python -m pytest -q -m integration sdks/wyrd-sdk-python/tests/integration/test_verifier_building_blocks.py::test_typed_verdict_data_mismatch_keeps_original_result
```

TypeScript, selecting exact Vitest titles in the existing file:

```bash
with_verification_postgres pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/verifier-building-blocks.test.ts -t '^custom_verdict_data_survives_readback$'
with_verification_postgres pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/verifier-building-blocks.test.ts -t '^invalid_custom_verdict_data_is_inconclusive$'
with_verification_postgres pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/verifier-building-blocks.test.ts -t '^custom_verdict_data_is_typed_on_return$'
with_verification_postgres pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/verifier-building-blocks.test.ts -t '^typed_verdict_data_mismatch_keeps_original_result$'
```

### HTTP and MCP proofs

All three names below are planned in the inspected existing modules. The HTTP
declaration journey submits invalid format/schema/contract cases for both check
kinds, asserts the existing structured validation refusal, and proves the
rejected Card was not persisted by reading its identity afterward. The HTTP
access proof includes authorized readback and other-tenant/refused reads; the MCP proof
discovers and calls the real tools, checks normal and invalid custom results,
and exercises scoped result/payload access. Retained data must also be checked
through the public Bifrost read path after publication, using the existing
server flush controls rather than sleeps or a new result API.

```bash
with_verification_postgres cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E 'test(=pg_verification_routes::verdict_declarations_validate_before_persistence)'
with_verification_postgres cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E 'test(=pg_verification_routes::custom_verdict_data_is_tenant_isolated)'
with_verification_postgres cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=verification::pg_tests::custom_verdict_data_round_trips)'
```

Existing focused regression commands, confirmed from source:

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::verifier::validate::tests::verifier_spec_requires_check_agents_to_produce_a_verdict)'
mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib -E 'test(=authoring::tests::output_schemas_are_validated)'
mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib -E 'test(=authoring::tests::native_body_pairs_and_round_trip)'
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::check_agent_keeps_only_valid_citations)'
```

After new focused proofs pass, run the affected contract/compiler, Agent,
client observe/verification, and building-block story regression scopes with
actual selectors recorded in execution evidence. Run Cargo commands
sequentially. Debug with `WYRD_LOG=info,vala_verify=debug` and read the trace.

Regenerate derived contracts with `mise run codegen:regen` and verify with
`mise run codegen:check`; author Python stubs and regenerate native TS
bindings/declarations through their owning builds. Run the affected checks once:

```bash
mise run fmt
mise run lints
mise run check:deps
mise run check:tenant-isolation
mise run py:format
mise run py:lints
mise run py:typecheck
mise run ts:format
mise run ts:lints
mise run ts:typecheck
mise run ts:napi:check
mise run docs:check:commands
mise run docs:linkcheck
mise run check:examples
mise exec -- git diff --check
```

Record RED/GREEN results, actual selectors, runtime/static typing evidence,
ordinary-call regressions, and final diff audit. Include tracked/untracked and
authored/generated consumers. Broad SDK/journey suites and documentation builds
run once at integrated change review, not as repeated task aggregates; do not
run `gate` for this task.

## Stop conditions and authority

Return a material conflict to spec authority if correctness needs different
public forms/calls, field placement, limits, failure/citation/privacy/capture/
retention semantics, a new dependency/feature or migration, a reversed tier
dependency, or replacement compiler/lifecycle/storage/API machinery. Likewise
stop if a provider cannot express the declared contract without weakening it,
or existing response bounds cannot support the accepted boundary without a
changed deployment/security contract. Finish unaffected work and provide source
evidence; routine setup, private fixtures, selector corrections, and recoverable
red tests remain implementation-owned.

Follow the [approved spec](../spec.md), [repository rules](../../../../AGENTS.md),
[execution discipline](../../../implementation-execution.md),
[spec workflow](../../../spec-driven-development.md), [testing map](../../../../TESTING.md),
[Python rules](../../../../sdks/wyrd-sdk-python/AGENTS.md),
[TypeScript rules](../../../../sdks/wyrd-sdk-ts/AGENTS.md),
[docs rules](../../../../docs/AGENTS.md), and owning architecture linked by the
spec. The task remains proposed; this editorial rewrite does not implement,
commit, approve, or assert readiness.

## Implementation evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| 1: named/plain/custom forms compile; invalid declarations persist no Card | `skald-spec` host-named formats (`d41aaf764`); `Verdict::response_format` and Verifier validation (`526e8ae19`) | `verifier_spec_requires_check_agents_to_produce_a_verdict`, `output_schemas_are_validated`, `native_body_pairs_and_round_trip`; `pg_verification_routes::verdict_declarations_validate_before_persistence` | PASS |
| 2: validation and inclusive bounds for both kinds; invalid output inconclusive with no data | `vala_verify::blocks::verdict_from`, `Verdict::check_data_bounds` (`526e8ae19`, `372e94026`) | `check_agent_keeps_only_valid_citations`; Rust/Python/TS `invalid_custom_verdict_data_is_inconclusive`; MCP `custom_verdict_data_round_trips` (BAD) | PASS |
| 3: data survives direct, queued, HTTP, MCP, Bifrost; foreign/scoped readers refused | Outcome Variant carries `data` unchanged | Rust/Python/TS `custom_verdict_data_survives_readback`; `pg_verification_routes::custom_verdict_data_is_tenant_isolated`; MCP `custom_verdict_data_round_trips` | PASS |
| 4: typed operations decode wholly or raise `WYRD_SDK_422_VERIFICATION_DATA_INVALID` | `decode_data` in `wyrd-spec` (`3cfc62f61`); TS `verifyTyped`/`waitTyped`/`statusTyped` (`3e60f4f97`); Python `verify_typed`/`wait_typed`/`status_typed` (`012535b78`) | `typed_data_decodes_whole_results`; Rust/Python/TS `custom_verdict_data_is_typed_on_return`, `typed_verdict_data_mismatch_keeps_original_result`; TS type tests; `py:typecheck` | PASS |
| 5: authoring, stubs, generated surfaces, docs, examples, and architecture agree | `068e0710e`, `6b6237961`, `012535b78` | `codegen:check`, `docs:generate` (no drift after commit), `docs:check:commands`, `docs:linkcheck`, `check:examples`; `support_desk_answers_verifies_and_explains_every_request`; `test_verdict_format_keeps_its_name_and_takes_a_model_for_data` | PASS |

Gates: `fmt`, `lints`, `check:deps`, `check:tenant-isolation`, `codegen:check`,
`py:format`, `py:lints`, `py:typecheck`, `ts:format`, `ts:lints`,
`ts:typecheck`, `ts:napi:check`, `git diff --check` pass. Non-goals stayed out;
no unrelated files were committed.

Limits and risks:

- RED not observed for scenarios 1–3: production behavior landed before
  the journeys that now pin it.
- Payload permission: verification outcomes, including custom `data`, are
  read through `bifrost_query_read` plus tenant scoping. `gateway_payload_read`
  gates only `vala.gateway.calls` payload columns. The MCP proof shows a role
  without query-read is refused. The spec's "elevated payload-read
  protections" wording does not match this documented posture; spec authority
  should confirm or narrow it.
- Python `ty` types `.get("data")` on a generic TypedDict as the default type;
  subscripting is typed correctly.
