# Verifier building blocks: execution and proof

Authority: [SPEC-verifier-building-blocks r8, approved](../spec.md).
The change has exactly five implementation tasks. This page coordinates their
seams, coverage, execution status, and final proof; it is not a sixth task.
Git staging, commits, worktrees, branch scheduling, and merges remain caller-
owned.

## Execution status — 2026-10-15

The owner approved Task 001 and its final remediation at
`a02055b5561b889fa9d934038109f3028b0ecb1d`. Tasks 002–004 can now execute
in parallel from that shared contract handoff. The owner approved Task 004
at `b4226148e9f962d0508ba46edb9c07676cad48ba`.

| Task | Status | Next step |
| --- | --- | --- |
| [001 — contracts](TASK-001-verifier-contract.md) | Complete; owner-approved | Shared handoff available |
| [002 — engine](TASK-002-verification-engine.md) | Ready | Execute alongside 003 and 004 |
| [003 — hosted runtime](TASK-003-hosted-check-runtime.md) | Ready | Execute alongside 002 and 004 |
| [004 — SDKs, examples, docs](TASK-004-sdks-examples-and-docs.md) | Complete; owner-approved | Journeys run at merged-change review |
| [005 — integration](TASK-005-server-integration.md) | Proposed; awaiting 002–004 | Compose their integrated result, then run final proof |

This records direct owner acceptance, not a new independent review PASS.
Prior immutable verdicts are preserved. Full integration and journey tests
remain due after all five tasks are merged.

The owner approved r8 on 2026-10-15: Task 005 owns the standalone
`wyrd-example-ts` refactor and new `wyrd-example-py`/`wyrd-example-rs`
repositories (REQ-016).

The owner approved the Task 004 handoff as r7 on 2026-10-15. Task 005 owns
the execution-based MCP/binding/dispatch cutover, existing Operator plumbing
and template adaptation, published SQL fields, affected lineage closure, and
connected-handle status/wait journey proof. Earlier task/review records retain
their original r6 revision; their acceptance is not rewritten by this handoff.
Coordinate SDK/docs/generated changes with Task 004. Multi-check drift-count
semantics require explicit approval if existing context is ambiguous; they
must not be guessed during integration.

## Dependencies and parallel work

```text
TASK-001  authoring, wire contracts, narrow existing execution seams
    |
    +--> TASK-002  reusable Vala engine -------------------+
    |                                                     |
    +--> TASK-003  hosted Agent/extraction/code runtime ----+--> TASK-005
    |                                                     |    compose server,
    +--> TASK-004  shared client, three SDKs, examples/docs -+    queue, live consumers
                                                               |
                                                    merge all five changes
                                                               |
                                                 full integrated proof once
```

Task 1 must publish actual typed boundaries and compilable staged exports,
not just a schema wish list. It does not implement fake production runtime
stubs. Tasks 2, 3, and 4 branch from that common contract and have no dependency
on each other's production implementation. Task 5 starts from their integrated
result and supplies live server composition and final old-contract cutover.

Each module keeps explicit injected dependencies and independently callable
operations. Test doubles support isolated RED/GREEN for a module; they never
substitute for the required real SDK/server and persistence evidence.

## Ownership and seam handoff

| Owner | Parallel-wave ownership and handoff |
| --- | --- |
| Task 1 | Foundation contracts, media vocabulary, error/schema sources, staged exports, and minimal existing Vala executor/invoker signature changes. Handoff records reference positions, cause/outcome rules, resolved-input/provenance/deadline shapes, public messages, and the exact final live switch points. |
| Task 2 | Vala driver/context/registry and deterministic/statistical composition, Vala root exports and its crate manifest. No hosted tools, workspace implementation, server lifecycle, or SDK logic. |
| Task 3 | Existing invoker internals and hosted evidence/extraction/tools/sandbox. Own server/runtime dependency selection and workspace manifest/lockfile changes. Coordinate any Vala root export with task 2; do not change task 1's contract privately. |
| Task 4 | Shared client, language wrappers/exports/stubs/declaration sources, matched example assets, docsite, and architecture/diagram prose. Own authored documentation while other tasks produce implementation facts. |
| Task 5 | Server/control, registry/bindings, SQL/migrations, Scribe/table/ACK/outbox, queue/runner/schedule/baseline/result composition, HTTP/MCP/CLI/UI cutover, the three standalone example repositories, and final obsolete-code removal. Regenerate final served/generated contracts and coordinate SDK/doc fixes through their actual owner. |

Paths in tasks identify behavioral owners, not strict private file allowlists.
One shared Cargo.lock or root-manifest update must be integrated before another
owner regenerates it. Do not run concurrent Cargo builds/tests/lints/codegen
against a shared target directory; independent source work is parallel, while
Cargo-backed verification is serialized or uses truly isolated targets.

Temporary retention of old symbols keeps intermediate branches buildable; it
is not a public compatibility layer. Only task 5 switches live consumers,
removes old kinds/routes/observation activation, and deletes remaining staged
scaffolding. No release/merge-ready claim is made with two live contracts.

## Coverage ownership

Every requirement and invariant remains mandatory. The primary owner delivers
the behavior; task 5 integrates it rather than reimplementing it.

| Spec obligations | Primary owner | Cross-owner closure |
| --- | --- | --- |
| REQ-001–003: authoring, slots, references and graph | Task 1 (contracts/validation), task 2 (execution) | Tasks 4/5 project and register/resolve the same contract |
| REQ-004–006: actions, checks, skips, aggregation | Task 2 (driver/assert/trace/drift), task 3 (Agent/extract/code) | Task 5 injects concrete capabilities; task 4 reads results |
| REQ-007–010: Agents, workspace, citations, media | Task 1 (pure contract), task 3 (host execution), task 4 (upload projection) | Task 5 authorizes/resolves exact evidence and composes |
| REQ-011–013: verify/submit, identity/status, batched lifecycle | Task 4 (client), task 5 (server/queue) | Task 1 locks common types and r6 asynchronous conflict semantics |
| REQ-014: drift windows, baselines, reactions | Task 2 (scoring adapter), task 5 (data/lifecycle integration) | Task 1 maps bindings; task 4 emits samples and explains results |
| REQ-015: attributable result | Tasks 1/2/3 (contract/content), task 5 (retention/status) | Task 4 projects and documents it |
| REQ-016: matched examples and standalone example repositories | Task 4 (in-repo examples), task 5 (example repositories) | Task 5 supplies real-server completion evidence |
| REQ-017: docsite, architecture, all consumers | Task 4 (docs/examples/SDKs), task 5 (HTTP/MCP/CLI/UI) | Task 1 supplies generation sources; final drift/build checks |
| INV-001/002: durable owners and one shared client | Tasks 1–5 within their assigned boundaries | Dependency checks and final composition review |
| INV-003/004: tenancy, authority, audit and durability | Task 3 (capability enforcement), task 5 (admission/queue/storage) | Task 4 preserves context/errors; real negative journeys |
| INV-005/007: read/compute only, bounded isolation | Tasks 2/3 (engine/runtime), task 5 (admission/lifecycle) | Focused containment proof plus real integrated refusals |
| INV-006: honest evidence | Tasks 1/2/3/5 | Task 4 retains result meanings and cites them correctly |
| INV-008: complete replacement | Task 5 | Task 4 migrates SDKs/examples/docs; preserve historical evidence |

| Acceptance criterion | Owner of strongest evidence |
| --- | --- |
| AC-001 | Task 1 isolated contracts; task 5 actual registration/served schemas |
| AC-002 | Task 2 isolated engine behavior |
| AC-003 | Tasks 2/3 isolated capabilities; task 5 integrated invocation |
| AC-004 | Task 3 real isolated sandbox/worker proof; task 5 production composition |
| AC-005 | Task 4 authors three-language journeys; task 5 integrated execution |
| AC-006 | Tasks 1–4 author their local negative branches; task 5 real cross-boundary negative/edge proof, including asynchronous replay conflicts |
| AC-007 | Task 2 drift composition; task 5 actual windows/baselines/queue/Operator integration |
| AC-008 | Task 4 owns in-repo matched runnable examples; task 5 owns the example repositories and final real-server proof |
| AC-009 | Task 4 typing/docs/examples/architecture; task 5 served OpenAPI/MCP and live consumer cutover |
| AC-010 | Each task focused proof; final integrated checks executed once after all five merge |

## Required action and check coverage

Every row below is mandatory, including reused implementations. Task 4 authors
real SDK → server → SDK journeys for Rust, Python, and TypeScript; task 5
composes the real capabilities and runs those journeys only after all five
tasks are merged. A scripted executor or mocked transport does not close a row.
A journey may cover several rows; do not create a separate demo per primitive.

| Position / kind | Isolated proof owner | Required merged behavior |
| --- | --- | --- |
| Action `extract` | Task 3 | Each of the nine supported formats returns its exact REQ-004 output; a downstream requirement consumes it. Invalid/malformed or over-limit evidence fails safely and cannot turn dependent work into a pass. |
| Action `agent` | Task 3 | The real Agent loop uses declared inputs/workspace/tools and returns its declared data schema; downstream work reads that output. Invocation/schema/timeout failures retain the approved failure and dependency semantics. |
| Action `code` | Task 3 | The real sandbox returns JSON consumed by downstream work. Exceptions, timeouts, and forbidden access are bounded, observable, and cannot pass dependent requirements. |
| Check `assert` | Task 2 | Real input and step-output comparisons produce passed and failed verdicts; missing required evidence is inconclusive. |
| Check `trace` | Task 2 | Real stored trace evidence exercises span existence, ordering, and attribute checks, including a failed expectation and unavailable required evidence. |
| Check `agent` | Task 3 | A tool-less Agent judges explicitly bound evidence and returns a cited passed/failed Verdict. Invalid/missing citations and invalid output cannot manufacture a pass. |
| Check `code` | Task 3 | The real sandbox returns passed/failed Verdicts with valid evidence citations. Invalid Verdicts, exceptions, forbidden access, and timeouts follow the approved failure semantics. |
| Check `drift` | Task 2; task 5 owns windows/baselines | PSI, SPC, and Custom work through actual scheduled bindings and requirement-specific fitted state; cover passing/failing drift and inconclusive insufficient/unready data. |

For each language, record the actual journey test names and exact focused
commands against these rows. Task 5 records their final passing evidence and
confirms repository lane discovery. This is a completion checklist, not a claim
that any test already exists or passed. Check placement, mixed-action graphs,
skips/aggregation, immediate versus queued behavior, tenant refusals, and
failed-only reactions remain required by the existing acceptance obligations.
The example app alone does not substitute for this complete coverage.

### Full scheduled drift lifecycle: extend the existing journeys

Reuse/refactor the existing stories instead of creating a second drift harness:

- Rust: `sdks/wyrd-sdk-rust/tests/integration/scheduled_drift_alerts_operator.rs`.
- Python: `sdks/wyrd-sdk-python/tests/integration/test_scheduled_drift_alerts_operator.py`
  and its `latency_watch` setup in `tests/integration/conftest.py`.
- TypeScript: `sdks/wyrd-sdk-ts/wyrd/tests/integration/scheduled-drift-alerts-operator.test.ts`.
- Shared assets: `fixtures/cards/scheduled_drift_alerts_operator` and
  `fixtures/cards/latency_baseline`. Keep the existing Service-based story;
  a Model component is optional. Existing `verify_in_real_time` journeys
  provide separate Model/component coverage.

Source inspection confirms the existing scheduled stories register a Service,
Data baseline, PSI Verifier, schedule Trigger, and HTTP Operator connection;
wait for the actual baseline fitter; emit through old `observe.drift`; drain
and publish observations; make the binding due; and receive an authenticated
HTTP delivery. The Service has no Model component, and the stories do not
explicitly read back the failed scheduled result. A Model is not required
for this lifecycle. Model drift is exercised
separately by the real-time verification stories. These are reuse candidates,
not a claim of passing runtime evidence.

Task 4 extends the same story in all three SDKs; task 5 composes and runs it
on the merged candidate. Required end-to-end sequence:

1. Through the real SDK, register the baseline data and a Service with a declared
   drift verification binding, schedule, and `on_failure` Operator. Preserve
   exact subject and Verifier identities. A Model component is optional.
2. Let registration enqueue baseline fitting and let the real background
   fitter compute it. Wait with a bounded deadline and verify readiness;
   never seed fitted state or compute it in the client/test. Supporting seam
   proof pins the asynchronous registration/fit boundary without relying on
   winning a race to observe a transient pending state.
3. Submit observations for the bound subject through `observe.submit`.
   Assert `Recorded` semantics, drain/flush the existing pipeline, and prove
   submission itself did not run the drift check or invoke the Operator.
4. Make the declared schedule due using the existing harness control; the real
   scheduler, queue, and runner must select the fixed window and fitted baseline.
   Do not replace this with direct `verify` or a hand-called scorer.
5. Read the retained failed result through the real client and assert subject,
   Verifier/requirement, window, baseline provenance, and drift statistics.
6. Receive the real Operator HTTP delivery on the existing local receiver;
   assert the configured connection/authentication and attribution to that
   failed execution. Retried/replayed work must not duplicate logical dispatch.

Also retain/prove the passing window and insufficient-data/unready-baseline
cases: they must not invoke the failure Operator. PSI/SPC/Custom coverage in
the matrix remains required; the complete fitted-baseline lifecycle uses a
method that actually requires fitting, rather than treating Custom as proof
of asynchronous fitting. These journeys execute after all five tasks merge.

### Startup-configured pool limits

Spec r6 Deployment limits owns exact environment names/defaults and admission
semantics. Task 3 proves the actual worker/pool/security limits; task 5 owns
startup parsing/validation and existing-queue capacity coordination; task 4
owns docsite parity. Pool defaults are 2 warm / 8 maximum workers sharing a
2 GiB worker-group ceiling. The 16-workspace cap is separate from both the
worker count and the 16-waiter cap. Final merged proof exercises configured
saturation and queue behavior without bypassing runtime admission.

### Built-in Agent tools and runtime authority

Task 3 owns REQ-008's built-in tool registry and automatic name resolution in
the hosted Invoker, extending existing Skald dispatch and authorized Rust
capability owners. Task 1 preserves declarative tool names; task 4 supports
registered built-ins in SDK authoring/docs and authors real tool journeys;
task 5 composes the registry and proves actual permissions/audit and accepted
authority through queued retry/restart. Every protected call retains the
original subject or valid narrowed delegation; SYSTEM is attribution, not
extra authority. Every tool first needs invoke permission under Wyrd's general global or
exact scope permission rule;
underlying capabilities independently check domain/resource access. Neither
gate implies the other, including for code and filesystem tools. Task 1 owns
typed grant coverage/intersection; tasks 3/5 prove enforcement and audit. Global tool availability never shares invocation credentials,
workspace, or mutable principal state. Agent checks remain tool-less.

## Isolated task proof

Follow each task's ordered Behavior / RED / GREEN / REFACTOR scenarios.
For every new executable behavior, collect the actual test name after adding
it and run its exact focused repository command before GREEN. Record package,
target, features, selector, failure reason, and result. Do not invent a
function name in a plan and assume an empty selection passed. Rust uses:

```text
mise exec -- cargo nextest run --locked -p <actual package> --lib \
  -E 'test(=<actual fully qualified test name>)'
```

The placeholders are execution-record instructions, not runnable commands or
claims that a future named test exists. For a real-worker component target,
use its actual owning test target instead of `--lib`. Existing module/package
commands in task files are owner-level proof; narrow RED/GREEN to actual
cases. Full environment-owning integration and journey suites are deferred,
not deleted, weakened, silently skipped, or replaced by mocks.

New Python/Node lifetime behavior runs in its owning language runtime. Pure
contract/engine behavior does not need a database. Hosted capability tests
can use local evidence and real isolated workers without booting Wyrd; do not
mislabel such IO/process proof as an IO-free unit test. Failed tests must be
rerun with tracing as required by AGENTS before diagnosis.

Docs/generated/static-only obligations use type, schema, link, rendering/build,
and source consistency proof without a manufactured RED. Static examples and
schemas cannot prove tenant isolation, database atomicity, accepted authority,
or a complete working demo.

## Full proof after all five changes are merged

This change intentionally crosses foundation, Vala/Skald, server/SQL/Bifrost,
three SDKs, CLI/UI, docs, and build/package consumers. No single capability
lane covers that complete closure. Use the repository aggregate once:

```bash
mise run gate
```

Do not also prescribe/rerun its component lanes as final checks. The inspected
`mise.toml` aggregate covers format/Clippy, dependency/tenant/storage/codegen
checks, docs/example builds, all Rust families, Rust SDK/installed-CLI
journeys, and Python/TypeScript integration suites. The server family includes
its integration target; `wyrd-testing` is a server dev dependency and enables
`wyrd-server/test-support`, which exposes the verification runtime module.
`test:bifrost:gate` itself selects ignored journeys only, so it is not the
source of that server-runtime coverage. It also covers assembled-
server OpenAPI through the server family target; MCP needs its actual owning
runtime tests, not an assumption that code generation proves it. Keep new
integration/journey tests in those real owning targets/lane selections.

Before that run, task 5 must verify test discovery against the final tree and
confirm the deferred coverage below is actually selected. If an existing
filter omits required new behavior, use the existing target/lane registration
pattern or record the minimum additional exact command; never assume gate
includes an unregistered or feature-disabled target.

| Required merged proof | Existing owning locations inspected during planning |
| --- | --- |
| Registration, endpoints, authorization, replay/conflict, status | `wyrd-server/tests/integration/main.rs`: `pg_card_registration_route`, `pg_verification_routes`; SQL integration target; real HTTP/MCP projection |
| ACK/queue, caller authority, restart, settlement/loss, Operators | Existing `pg_verification_runtime` test-support module and verification fixtures/fault controls; scheduled drift and Operator journey owners |
| Real Rust workflow | `sdks/wyrd-sdk-rust/tests/integration/main.rs`: real-time verification, support desk, observation and scheduled-drift stories; package is `wyrd-ai`, target `integration` |
| Real Python workflow | `sdks/wyrd-sdk-python/tests/integration`, using public `wyrd` imports and integration marker |
| Real TypeScript workflow | `sdks/wyrd-sdk-ts/wyrd/tests/integration`, using public `wyrd-ai` and repository-managed `@wyrd/testing` |
| Served contract and agent tools | Existing `pg_openapi_contract` and owning MCP runtime/schema tests, including new execute/enqueue/evidence/status behavior |

These names identify current module owners, not newly prescribed test
function names. New function names and exact selectors belong to the actual
implementation/evidence record. First full-suite execution happens only after
task 5 too is merged. Failures require diagnosis and focused reruns; "once"
does not permit leaving a red test or skipping the eventual passing proof.

### Proof outside the repository aggregate

`gate` builds example binaries but does not run the three live support-desk
example tasks, and it does not enter the separate `wyrd-example-ts`,
`wyrd-example-py`, or `wyrd-example-rs` repositories.
These are the only planned extra final obligations; do not duplicate the
aggregate's SDK/docsite lanes.

With the server, tenant, registered components, and provider setup supplied by
the task 4/5 instructions/repository-managed fixture (never production secrets):

```bash
mise run examples:rust:support-desk
mise run examples:python:support-desk
mise run examples:typescript:support-desk
mise exec -- npm --prefix ../wyrd-example-ts run typecheck
mise exec -- npm --prefix ../wyrd-example-ts run test
mise exec -- uv run --directory ../wyrd-example-py pytest
mise exec -- cargo test --manifest-path ../wyrd-example-rs/Cargo.toml
```

Each example repository's test starts its own application but expects the
Wyrd server to exist. Task 5 must document/compose that prerequisite and the
real SDK package build; do not run it blind or count a missing-service failure
as proof of behavior. Its startup/shutdown and expected failing-answer checks
remain mandatory. Use a deterministic local provider for automated proof;
live-provider demo setup is documented separately and does not introduce a
credential requirement into unit tests.

If a task's isolated worker containment proof uses a new external target,
ensure that target is also discovered in the final existing owner lane. Do not
add a new permanent repository check merely to count these task files or
validate this packet.

## Completion evidence

Each task records source changes, exact isolated RED/GREEN/REFACTOR commands,
static proof where appropriate, and the shared acceptance obligations still
awaiting integrated evidence. Do not label deferred journeys as passed.
Task 5 records the merged candidate and maps every REQ, INV, and AC to the
strongest final proof, including the three examples and external app.

Task implementation, independent review, and direct owner acceptance are
recorded separately. The execution status above reflects the owner's approval;
it does not claim the integrated change has passed or authorize a merge or
publication. No remaining work may be hidden behind "pre-existing failure"
or waived for the five-task count.
