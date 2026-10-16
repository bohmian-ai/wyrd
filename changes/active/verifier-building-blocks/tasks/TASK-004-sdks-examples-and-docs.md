---
id: TASK-004
kind: implementation
status: ready
spec: SPEC-verifier-building-blocks
spec_revision: 6
requirements: [REQ-001, REQ-002, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, INV-001, INV-002, INV-003, INV-004, INV-006, INV-008, AC-005, AC-006, AC-007, AC-008, AC-009, AC-010]
depends_on: [TASK-001]
---

# One developer workflow in Rust, Python, and TypeScript

## Outcome and Value

Developers can use the same verify/submit workflow, inputs, typed breakdowns,
and execution handle in all three SDKs. Matching runnable examples, the
TypeScript demo, docsite guides, and architecture explain that same contract.
The client module is testable against contract responses without waiting for
the new server engine or hosted checks.

## Owners, Scope, Consumers, and Prohibited Changes

Own `wyrd-client` projection and storage-upload reuse; the Rust/Python/TypeScript
SDK wrappers, authored typing, generated declaration sources, exports, and
language-runtime tests; shared support-policy example assets and all three
entry points; `wyrd-example-ts` (moved to TASK-005 by r8); docsite and architecture updates including
the HTML flow diagram. Task 5 owns HTTP/MCP/CLI/UI live-server cutover.

Implement client behavior once in the shared Rust owner. Python and TypeScript
only add their legitimate foreign-runtime ergonomics and error projection.
No permission decisions, durable queue state, polling server, provider engine,
or new registration/storage lifecycle belongs in an SDK.

Work after task 1, in parallel with tasks 2 and 3. Use typed wire responses and
existing transport/native doubles for isolated proof. Author the required real
SDK/server journeys and update old ones now, but execute full journeys and
integration suites only after task 5 joins all modules. Do not claim the
examples/demo have run end to end until that merged proof exists.

## Source-backed Reuse Map

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
| --- | --- | --- | --- | --- | --- |
| Shared observation projection | `wyrd-client/src/observe/mod.rs::Run`, `Observe`; `verify.rs::direct_input`, `execute`; lifecycle/Drift projection | `observe/tests.rs` uses local transport responses; real-time verification journeys in all SDKs | Kind-specific input and separate eval/drift emits; no generic submit/wait | Extend the existing observe owner and retained WyrdClient with typed slot projection, submit, polling, and error handling | Execution handle is approved public behavior; no second transport or durable client engine |
| File transfer | `WyrdStorageClient::upload_artifact`, `storage/upload/mod.rs::dispatch`, SinglePut implementation | Existing upload validation/dispatch callers and current Observe projection | No lazy evidence handle and plan-without-Card workflow | Expose/use the existing byte-transfer capability for File/ArtifactRef declarations | A small storage executor extension is justified if existing entry point assumes Artifact completion; do not duplicate PUT transport |
| Language boundaries | Python `wyrd.observe`/state wrappers and authored stubs; TypeScript Observe wrapper/native binding; Rust `wyrd-ai` exports | Python `tests/unit/state/test_observe_surface.py`; TS `tests/unit/observe.test.ts`; existing three-language real-time journeys | Generic inputs, Submission, rehydratable execution handle and wait missing | Project shared Rust operations with existing connect/ambient-client/error conventions | Thin runtime wrappers only; no Python/TS replay or queue policy |
| Runnable story and docs | `examples/support-desk` deploy/serve/wait/explain; same Cards in all three languages; verification guides | Rust support_desk journey imports checked-in example; Python/TS matching journeys; demo `test/ask.test.ts` | Old continuous Eval story; no cited policy-file preparation and queued review | Extend the same story/assets and demo; revise existing guides and architecture sources | No new app framework or separate example semantics |

## Approach

1. Consume task 1's wire/media handoff and extend the shared client with named
   projection, lazy file hashing/upload, verify/submit, and status/wait.
2. Project those operations in all three SDKs, preserving typed errors,
   construction, optional client, and language-runtime ownership.
3. Add focused client/wrapper TDD; author corresponding real-server journeys
   and migrations of existing fixtures without running their full suites.
4. Extend shared support-policy assets and all three examples; replace the
   demo's temporary SDK symlink with a reproducible built package.
5. Update docsite, generated references, and architecture to spec r6; provide
   final runnable evidence instructions and deferred proof inventory to task 5.

## Ordered Implementation Scenarios

### Scenario 1 — Ordinary values reach the right named slots

**Behavior.** JSON, trace, File/path/bytes, and ArtifactRef project from the
hydrated Verifier; unknown names/types and oversize inputs fail safely.
Constructing File is local/lazy; hashing and SinglePut use the storage owner,
not Card registration or a second HTTP client (REQ-002/010/011).

**RED.** Add focused shared-client tests using existing transport/queue test
facilities and local input data. Show missing named projection, lazy behavior,
or upload-plan handling, recording actual exact selectors.

**GREEN.** Extend the existing Observe/storage projection and preserve exact
subject/Run correlation and safe errors.

**REFACTOR.** Remove duplicate value conversion/upload code; keep slot semantics
in shared Rust and ordinary data ordinary.

### Scenario 2 — Submission and waiting have one honest contract

**Behavior.** Run submit returns a queued handle; Window submit records samples
and returns Recorded; verify refuses Window. Polling tolerates missing rows
and delayed results within its timeout, fails on terminal errors including
asynchronous idempotency conflicts, and reconnects through existing client
construction (REQ-011–013/015).

**RED.** Add focused shared-client tests with scripted status responses and
controlled time. Include completed-without-result, lost staged input,
transient failures, terminal conflict, identical retry key reuse, and retained
explicit/ambient client. No database or full service is needed.

**GREEN.** Implement the approved routes, return union, stable retry-key use,
bounded polling, and error projection over the existing WyrdClient. Rerun
scenario 1. Do not promise synchronous replay acceptance from 202.

**REFACTOR.** Keep retry/polling policy shared; no per-language status cache,
background runtime, or hidden second connection.

### Scenario 3 — Each language exposes the same usable behavior

**Behavior.** Rust, Python, and TypeScript export typed inputs/results/handles
and matching sync/async conventions where already supported. Wrong inputs,
Recorded versus queued behavior, and stable Wyrd errors preserve semantics;
no obsolete emit aliases remain in the final surface (INV-002/008). Built-in
tool names survive authoring/registration in all SDKs without local callable
registration; arbitrary client callables remain non-registrable. Add a hosted
Agent journey that uses declared Card/Bifrost and code tools, including denied
and delegated access, for the final merged pass.

**RED.** Add focused Rust export/compile checks, Python runtime tests, and
TypeScript wrapper/native tests against the shared operations. Run Python-
lifetime behavior in Python and Node-lifetime behavior in Node, never Rust.

**GREEN.** Complete wrappers, registration, exports, authored Python stubs,
and generated TypeScript declaration sources. Rerun earlier client tests.

**REFACTOR.** Remove duplicated boundary logic and preserve existing connect,
ambient client, token reuse, and error metadata.

### Scenario 4 — The same policy-check story is runnable and documented

**Behavior.** Author real journeys for every row of the README action/check
coverage matrix in Rust, Python, and TypeScript, including reused behavior.
Shared Cards/policy evidence support matching Rust/Python/TS
examples and the real TypeScript app: file-reading Agent step, original
citations, deterministic refund assertion, tool-less judge, immediate and
background flows, resume by ID, and failing answers (REQ-016/017).

**RED.** For new executable example/demo behavior, add focused local tests of
request adaptation and result presentation with existing typed test doubles;
confirm the missing behavior. Author the actual SDK/server journey assertions
with these examples; their execution is deliberately deferred to the merged
candidate. Documentation itself needs no manufactured RED.

**GREEN.** Extend the existing examples/demo and shared assets. Complete
setup/run instructions, including real SDK build and deterministic automated
provider setup. Keep the same requirements and expected meanings in every
language; do not replace final real-server proof with local doubles.

**REFACTOR.** Reuse the canonical story and avoid copied divergent YAML,
new app frameworks, and needless example helpers.

### Scenario 5 — Existing drift journeys cover the full scheduled lifecycle

**Behavior.** Extend the existing scheduled-drift/Operator stories and shared
fixtures identified in README. All three SDKs register the Service with
declared drift, wait for real asynchronous baseline fitting, record
observations for the bound subject through `observe.submit`, and observe a scheduled failed result
and real Operator delivery. A Model component is optional. Passing and
inconclusive cases do not dispatch.

**RED.** Adapt the existing journey assertions to the new contracts and add
bound-subject identity, Recorded semantics, retained result/provenance, and
no-early/no-duplicate delivery assertions. Compile/typecheck them now; first
full execution remains deferred until all five tasks merge. Record actual
focused selectors then; do not label a transport double as journey evidence.

**GREEN.** Migrate the existing fixtures and SDK story to the approved bindings
and submission API, preserving the real fitter, scheduler, and HTTP receiver.
Keep the Service-based fixture and document the same lifecycle; existing
Model registration coverage remains separate.

**REFACTOR.** Share existing fixture assets across languages; remove obsolete
`observe.drift` usage without adding a second drift harness or client fitter.

## Contract Handoff From TASK-001

The LLM judge is not kept (owner decision); judged requirements use an
`agent` check with the Verdict schema. Remove or rewrite every judge surface in
this task's write set, with no alias: `eval_llm_judge`/`task_llm_judge` in
`sdks/wyrd-sdk-python/python/wyrd/observe/__init__.pyi`; judge types in
`sdks/wyrd-sdk-ts/wyrd/src/card-types.ts` (regenerate, do not hand-edit);
judge cases in `sdks/wyrd-sdk-rust/tests/integration/{verify_in_real_time,gateway_jev}.rs`,
`sdks/wyrd-sdk-python/tests/integration/{test_verify_in_real_time,test_gateway_jev}.py`,
`sdks/wyrd-sdk-ts/wyrd/tests/integration/{verify-in-real-time,gateway-jev}.test.ts`;
fixtures `fixtures/cards/verify_in_real_time/{answer-judged-off-route,answer-is-judged,answer-graded}.yaml`,
`fixtures/cards/gateway_jev/reply-quality.yaml`, `fixtures/README.md`;
`examples/support-desk/service/answer-quality.yaml`; docs
`build-and-verify/verify/{evaluate-agent-behavior,verify-a-request-in-real-time}.md`
and `understand/how-verification-works.md`. Engine, server, CLI, and
contract removal is TASK-005's (map there).

## Acceptance Criteria

- All three SDKs use the same shared client behavior and typed errors.
- Ordinary named inputs, lazy evidence upload, submit routing, replay-key use,
  status, wait, and reconnect have isolated test coverage in their owner.
- Real journeys explicitly exercise all three action kinds and all five check
  kinds in every first-class language, as listed in the README coverage matrix.
- Real journeys are authored for every exposed language and negative/edge flow;
  their integrated execution remains an explicit outstanding shared obligation.
- Matching examples/demo and all required architecture/docsite updates exist;
  instructions accurately describe asynchronous acceptance and result loss.
- The docsite reference publishes every locked REQ-004 output type with a JSON
  example per format, assertion/reference examples, required/null and ordering
  semantics, table/cell behavior, source locators, and visual-input limits. All
  three SDKs preserve the same JSON result; no language-specific parser model
  or extracted-image helper is introduced.
- Complete the planned docsite verification-runtime configuration reference:
  all environment names/defaults/units, startup validation/restart, pool scope,
  deadline-based bounded waiting, and memory/security limits. Confirm parity
  against the final startup configuration before presenting it as available.
  Document the REQ-008 built-in catalog, automatic resolution, no client
  registration requirement, both tool-grant wire forms, and the independent
  tool-invoke/domain-resource gates under original-subject/delegated authority.
  Update existing typed permission projections and role/grant consumers for
  exact tool scope; no client-only authorization logic.
- Generated artifacts are source-driven; old kind/Workflow/observation prose
  is removed from active documentation rather than left as an alternate flow.

## Expected Write Set and Consumer Closure

`crates/shared/wyrd-client` observe/storage/state integration; all three
`sdks/wyrd-sdk-*` packages, Python native aggregation/exports/stubs, TS native
and authored wrapper/declaration sources; owning unit and journey files;
`examples/support-desk`, its tasks when needed; `docs` guides/navigation and
source-generated references; architecture/reference/AGENTS contract prose;
`changes/active/verifier-building-blocks/architecture/verification-flow.html`;
external `wyrd-example-ts` source, service Cards, tests, package/build setup
(moved to TASK-005 by r8).

Own authored SDK/docs/example changes during the parallel wave. Coordinate
contract generator publication with task 1 and final served-schema regeneration
with task 5. Do not redesign the UI or diagram; update affected contract text.

## Verification and Evidence

Use TDD for executable projection/wrapper/example behavior. Use static links,
typing, schema generation, and doc builds for prose/generated obligations.
Run focused tests in each runtime after rebuilding only its touched binding.
Current target/module owners were inspected; actual new test names are recorded
and exact-focused at RED time.

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(/^observe::/)'
mise exec -- cargo clippy --locked -p wyrd-client --all-features --lib -- -D warnings
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state/test_observe_surface.py
mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/unit/observe.test.ts
mise run py:format
mise run py:lints
mise run py:typecheck
mise run ts:format:check
mise run ts:lints
mise run ts:typecheck
mise run codegen:check
mise run docs:check
mise run check:examples
mise run fmt
mise run check:deps
```

The Rust crate/package, Python path, and Vitest owner above exist in the base;
these are owner slices, not newly invented test selectors. Do not run full
SDK integration/journey suites or the demo's current live-server test target
before all tasks are merged. The final coordination document records those
proofs once, rather than repeating every language suite in each task.

## Material Stop Conditions

No Python/TS durable behavior, alternative transport, required-client API,
new UI-only workflow, weakened replay semantics, compatibility alias, or mock-
only final demo is authorized. A cross-boundary field/ownership change returns
to spec; ordinary wrapper/fixture/document organization stays local.

## Authority Links

- [Approved specification r6](../spec.md), [supporting proposal](../proposal.md),
  [worked examples](../example.md), and [flow diagram](../architecture/verification-flow.html).
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../architecture/agent-rules.md),
  [Wyrd design](../../../../architecture/wyrd-design.md), and
  [Bifrost design](../../../../architecture/bifrost-design.md).
- [Execution and final proof coordination](README.md). Ready after owner-approved
  completion of TASK-001; execute independently alongside the other parallel tasks.

## Implementation Evidence

Status: `IMPLEMENTED` against spec r7. Commits `52fc831f8`..`07adb7907` on
`change/verifier-building-blocks-task-004`. Nothing has shipped, so old
surfaces were deleted outright: no aliases, shims, or migrations.

Reuse: `wyrd-client` remains the only implementation. Its `verify` module
(`File`, `ArtifactRef`, `Input`/`Inputs`, `Submission`, `Execution`) and
`Observe::{verify, submit}` are projected unchanged by `wyrd-ai`, by
`wyrd.verify` (PyO3, `sdks/wyrd-sdk-python/src/verify.rs`), and by the TS
`verify` namespace (`native/src/verification.rs`). No SDK owns transport,
upload, validation, or polling. Window `submit` reuses the existing drift
projection (`observe/drift.rs`). The handle follows the design's
`connect`/`connect_with` convention.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Three SDKs share client behavior and typed errors | `crates/shared/wyrd-client/src/verify/`, `observe/mod.rs`; `wyrd-ai` re-export; `wyrd.verify`; TS `verify` namespace | clippy `-D warnings` (wyrd-client, wyrd-ai, wyrd-sdk-python, wyrd-sdk-ts; all features and lib-only); `ts:napi:check`; `codegen:check` | PASS |
| Named inputs, lazy upload, submit routing, replay key, status, wait, reconnect covered in owner | `crates/shared/wyrd-client/src/observe/tests.rs` (wiremock) | `nextest -p wyrd-client --lib 'test(/^observe::\|^verify::/)'`: 28 passed | PASS |
| Journeys exercise all action and check kinds in every language (README matrix) | `sdks/wyrd-sdk-rust/tests/integration/verifier_building_blocks.rs`, `sdks/wyrd-sdk-python/tests/integration/test_verifier_building_blocks.py`, `sdks/wyrd-sdk-ts/wyrd/tests/integration/verifier-building-blocks.test.ts`; lanes in `mise.toml` | compiled, linted, and typechecked; not run (see limits) | PASS (authored) |
| Negative/edge journeys authored; reconnect exercises `status()` and `wait()` (r7) | Same files plus `verify_in_real_time` and `action-timeline` journeys; cross-tenant reconnect refusal; status after wait equals the waited result | as above | PASS (authored) |
| Examples/demo and docs describe async acceptance and result loss | `examples/support-desk/{service,rust,python,typescript,README.md}`; `docs/src/content/docs/build-and-verify/verify/*`. External demo moved to TASK-005 (r8) | `check:examples`, `docs:check` | PASS |
| REQ-004 output reference | `docs/src/content/docs/reference/extraction-output.md` | `docs:check` | PASS |
| Runtime configuration, REQ-008 catalog, tool grants | `docs/src/content/docs/reference/verification-runtime-limits.md`, `reference/agent-tools-and-authorization.md`, `operate/grant-and-inspect-permissions.md` | `docs:check` | PASS |
| Generated artifacts source-driven; old flows removed | Workflow examples, judge/observation emits and Cards, `File`/`ArtifactRef` root exports removed; MCP table names the r7 tools | `codegen:check`, `check:deps`, `py:test:unit` export tests, `ts:test:unit` | PASS |

Commands run green: `cargo nextest` (above), `cargo clippy` (above),
`py:format`, `py:lints`, `py:typecheck`, `py:test:unit` (628 passed),
`ts:build`, `ts:format:check`, `ts:lints`, `ts:typecheck`, `ts:test:unit`
(70 passed), `ts:napi:check`, `codegen:check`, `docs:check`,
`check:examples`, `fmt`, `check:deps`, `git diff --check`.

Limits and TASK-005 handoff (approved r7):
- Journeys are authored, not run. They run once at change review after
  TASK-005.
- MCP `verification.execute/enqueue/evidence/get_execution` and removal of
  `start_run`/`get_run`: the docs already name them, but the server is TASK-005.
- Execution `dispatches`, binding `last_execution_id`, template identity
  cutover, REQ-015 SQL fields, and lineage semantics: SDK types follow once
  TASK-005 changes the spec types.
- Harness Verifiers need the new-form binding; check whether
  `fixtures/cards/gateway_jev/reviewer*.yaml` is still referenced.
- Not yet covered by journeys: agent schema/timeout and code-timeout refusals.
- Python support-desk deploy raises `LookupError`, whereas Rust raises
  `WYRD_GATEWAY_404_MODEL_UNAVAILABLE`.
- r8 moved the external example repositories to TASK-005; the uncommitted
  partial `wyrd-example-ts` update is discarded, not handed off.
