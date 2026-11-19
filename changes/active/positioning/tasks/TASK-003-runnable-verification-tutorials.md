---
id: TASK-003
kind: implementation
status: implemented
spec: SPEC-positioning
spec_revision: 5
requirements: [REQ-008, REQ-011, REQ-012, REQ-014, REQ-015, REQ-016, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, AC-007, AC-010, AC-011, AC-013, AC-014, AC-015]
depends_on: [TASK-001, TASK-002, TASK-002-R1]
---

# Publish runnable application and agent verification tutorials

## Outcome and Value

A reader can apply verification during a request, catch a skipped required
agent action, run a recurring server-side check, and investigate a failure.
Each workflow has executable source, observable success/failure and a proven
public path. A good-looking final answer must not conceal incomplete work.

The docsite already teaches verification independently of Wyrd, explains Wyrd's
mechanics, and shows the finance request handler in published production
practices. The finance API also has a real Python request journey. Complete
the remaining runnable setup, negative cases and proof; do not recreate that
application or repeat the conceptual lessons.

The request, background/evidence, drift and response drafts are reuse inputs,
not a fixed list of pages to publish unchanged. Preserve existing URLs and give
each affected page a reasoned disposition. Reuse useful steps at their existing
owner, replace stale snippets with tested source, and keep unrelated reference
material at its owner. Draft prose and passing leaf SDK tests alone do not
prove a reader can run the workflow.

## What Changes and What Must Stay True

Own the four tutorial outcomes, minimal local examples, journey closure,
rendered instructions and reciprocal concept/practice links. Each guide owns
one outcome; tutorials can share an example where that preserves independently
runnable steps and avoids copying a whole application. Record the tutorial
portion of the approved overlap handoff in the older docsite packet.

The existing draft guides present first-class language variants. Plan proof
for Python, Rust and TypeScript on each presented workflow, using existing SDK
journey targets. Language snippets must come from actual runnable examples.
Keep explicit prerequisites, real grant/tenant boundaries, current APIs and
stable errors. Operators and Triggers remain existing owners, not local
substitutes. Do not add a framework/dependency or require the full support-desk
application merely to run a small tutorial.

Actual workflow evidence must establish what ran. A missing or late trace alone
cannot establish a skipped action. Use adequate original evidence under the
existing input/check contracts; demonstrate complete, explicitly incomplete
and insufficient-evidence cases without fabricating application execution.
Evidence may contain synthetic domain data, but must be produced by the program
actually running its steps, not hand-authored labels posing as observed work.

No runtime capability, hot-path wait, schema, auth, migration or scheduler
change is authorized. If an advertised distinction cannot be supported by
current public contracts and adequate evidence, return to the spec. Do not
silently omit a required tutorial or publish an unproven procedure as a draft
and claim completion.

## Approach

1. Start from the current published corpus and the coverage map below.
   Revalidate declare → execute/schedule → inspect, record what already works,
   and identify only the missing tutorial steps and proof.
2. Run scenarios in order, adding only missing example behavior/proof and
   keeping useful existing negative tests. Reuse shared provider/harness support.
3. Teach one outcome per guide, embedding tested files and captured output;
   include only reader-relevant setup and concise failure recovery.
4. Follow the Task 2 remediation's learning sequence: Foundations and Real-time
   verification teach the concepts; How verification works in Wyrd owns product
   mechanics; these tutorials own executable procedures. Link rather than
   repeat those lessons or the production practices and FAQ.
5. Check rendered instructions against actual-program journeys, pin installs to
   the release shipping this change, regenerate indexes and hand Task 4 each
   canonical route, language scope, proof link and draft disposition.

## Ordered Implementation Scenarios

### Scenario 1 — Verify within a real application decision

**Behavior.** A request supplies evidence, the running application calls its
bound Verifier and receives the result before choosing its response/action.
A passing request proceeds; a violation, inconclusive result and typed error
use the documented hold/review response. The guide adds request-lifecycle
integration to the finance quickstart instead of copying its entire procedure. Lead with the current decision boundary:
verification finishes before the release/hold response or consequential action
is committed. Explain explicit requirement/evidence contracts and why evidence
retention is separate from the returned verdict. Present queued/scheduled
workflows as alternatives when a later judgment fits the decision, not as
prerequisites for the real-time path. Separate engine overhead, evidence work
and check/provider execution when discussing budgets; no numeric latency bound
is established by a scripted-provider journey.

**RED.** First run the existing finance API journey in
[live-request proof](#live-request-proof) and retain its pass/fail/inconclusive
coverage. Add only missing ordering, typed-error and presented-language proof
in the existing owners. Drive the actual small
application entry point and inspect the decision and result. Prove the
response/consequential action occurs only after the verdict, and that
non-pass/error paths never release, using observed ordering rather than
fragile elapsed-time thresholds. Fail for missing request/decision behavior, not compilation, missing assets or provider setup.
Already-correct portions may be verification-only with concrete observations.

**GREEN.** Extend `examples/finance-release/api.py` and the existing finance
example integrations only where the proof exposes a gap;
show per-request Run identity, requirement results and application handling.
Use scripted provider support for repeatability without asserting live quality.

**REFACTOR.** Share the existing example/declaration path and remove duplicated
request orchestration. Keep the guide independently runnable and all presented
language proof green.

### Scenario 2 — Catch skipped work despite a plausible answer

**Behavior.** Run a small agent workflow with a required action. Complete and
skipped-action executions can produce equally plausible final answers, but
their original execution evidence establishes different requirement outcomes.
Insufficient evidence is inconclusive/non-pass rather than a fabricated pass
or absence violation. Show the relevant requirement/evidence distinction.

**RED.** Add planned agent-workflow journeys to the existing building-block
owners in [workflow proof](#workflow-proof). Execute the actual workflow in
complete, skipped and insufficient-evidence cases. A final-answer-only check
must fail to establish the required behavior. Do not use a hand-built trace
as the only evidence that the new example actually executed its actions.

**GREEN.** Reuse the current small example and reader/provider harness where
appropriate; the existing support-agent reply checks do not establish that a
required action ran. Compose the current input, trace/assert/code check and telemetry
capabilities as appropriate. Capture evidence from the actual program, resolve
its completeness under existing semantics, and print the attributable result.
Keep declared requirements and domain data small; add no completeness oracle,
new tracing protocol or runtime observer. Rerun Scenario 1.

**REFACTOR.** Reuse existing instrumentation and check owners; remove any
parallel trace parser, evaluator or workflow framework. The example shows
verification of a workflow; Wyrd does not become the application runtime.

### Scenario 3 — Execute recurring verification on the server

**Behavior.** Declare a supported recurring Verifier binding and Trigger,
record controlled evidence through the public SDK, and observe the server's
scheduled result. Normal and violating evidence produce the intended distinct
judgments; insufficient evidence follows native non-pass behavior. Results
name the exact binding/subject/Verifier. No client loop substitutes for the
server scheduler. Operator delivery is taught only where needed by the guide.

**RED.** Add planned scheduled-example journeys in the existing schedule owners
in [schedule proof](#schedule-proof). They must drive the actual example and
read a result from the real scheduler; a printed “scheduled” label or manual
verify is insufficient. Existing scheduler behavior can be verification-only;
RED concerns missing reader-example/negative behavior, not clock flakiness.

**GREEN.** Reuse declared Trigger/binding, sample submission, baseline readiness
where applicable, scheduler and retained inspection. Harness-only due-time
controls may accelerate automated proof; public instructions must use the real
supported schedule and never call test helpers. Rerun previous scenarios.

**REFACTOR.** Remove ad hoc scheduling/polling-as-execution or baseline stores.
Bounded public result polling may observe completion, not perform the check.
Keep evidence draining and startup/shutdown faithful to current APIs.

### Scenario 4 — Investigate a judgment through retained evidence

**Behavior.** Start with an actual failed result, identify the failed
requirement, and inspect original evidence, citations and exact versions
through supported public execution/Bifrost surfaces. Explain the distinction
between the returned compact decision and retained details. Reads do not
rerun the model; evidence not yet visible is not fabricated or treated as
success. Preserve tenant isolation and native error handling.

**RED.** Add planned investigation journeys in the existing building-block
owners in [investigation proof](#investigation-proof), running the actual
example commands/inspection path. Assert attributable original evidence and
stable handling of unavailable/refused reads; fail for a missing usable reader
path, not an invented synchronous-publication guarantee.

**GREEN.** Reuse `Execution` status/details/evidence and existing Bifrost
inspection. Present real captured results and bounded supported visibility
handling. Complete the background/evidence procedure at its current route and
repair response links; keep Operator delivery guidance at its existing owner.
Rerun previous scenarios.

**REFACTOR.** Keep one inspection workflow through existing handles, without a
private endpoint, query client or result store. Preserve unrelated Operator
reference material; a simple investigation does not require notifications.

## Acceptance Criteria

| Obligations | Required observation |
|---|---|
| REQ-012; AC-011; INV-001–005 | All four actual tutorial programs execute through real SDK/server journeys on every language shown. Scenario-specific pass/violation and edge observations agree with the rendered tutorials. |
| REQ-011–012; AC-010–011; INV-002–003 | The skipped-step example proves adequate original execution evidence; missing evidence never silently proves completion or absence. Investigation distinguishes a result from later visibility and does not rerun judging. |
| REQ-008, REQ-014; AC-007, AC-013; INV-004–006 | Each tutorial has one reader purpose, a canonical route/disposition and accurate prerequisites/links; no duplicate finance procedure or invented keyword-only page. Query mapping is supplied to Task 4. |
| REQ-015–016; AC-014–015; INV-005–006 | Executed evidence/captured output support all tutorial claims and truthful attribution. Publication evidence identifies the skipped-step priority and full required tutorial completion, not only the initial landing set. |

## Implementation Reference

### Current coverage and remaining work

| Outcome | Reuse already present | Remaining tutorial/proof gap |
|---|---|---|
| Request decision | Finance quickstart; `examples/finance-release/api.py`; published production practice; Python actual-handler journey | Reader-complete request setup and output; explicit verdict-before-action observation; typed-error hold; actual-program proof for other presented languages. |
| Skipped agent work | Small `examples/support-agent` programs and reader/provider journeys; evidence-sufficiency practice; trace building-block journeys | Execute complete/skipped/insufficient-evidence workflows and judge original execution evidence. A policy-breaking reply is not a skipped-action demonstration. |
| Recurring verification | Draft drift procedure and existing real scheduler journeys | Independently runnable source, complete setup/readback and controlled scheduled outcomes. |
| Failure investigation | Draft background/evidence procedure and public Execution/Bifrost journeys | Complete example setup, attributable retained evidence, later visibility and refused/unavailable-read handling. |

- **Request lifecycle:** `examples/finance-release/api.py::send_report` uses
  the quickstart's `finance_service.py::Release`, one state, a Run and a
  release/hold decision. `test_docsite_quickstart.py` loads that actual file
  and asserts pass/fail/inconclusive HTTP responses (focused command below).
  `act-on-verdicts-in-production.md` embeds its handler;
  `how-verification-works.md` owns the request flow and timing explanation.
  `connect/integrate-asgi.md` owns framework lifecycle capture. Extend these
  owners rather than rebuild the refund draft or support-desk application.
  The Rust/TS finance programs are existing verification sources, not yet proof
  of request integration; existing real-time SDK journeys provide that seam.
- **Workflow evidence:** `examples/support-agent/support_agent.{py,rs,ts}`
  and `examples/tests/support_agent.rs`, Python
  `test_readme_support_agent.py`, and TS `support-agent.test.ts` already run a
  small agent and verify its reply. Reuse their reader/provider setup where
  appropriate; their reply-policy checks do not prove required action execution.
  Rust `verifier_building_blocks.rs::PolicyDesk::record_trace`, its
  `whole-trace.yaml`, and corresponding Python/TS journeys cover public trace
  check semantics. The `establish-sufficient-evidence.md` practice owns the
  completeness explanation. Add only the actual executed/skipped workflow and
  its evidence proof; manually supplied span fixtures are not that proof.
- **Continuous verification:** Rust
  `scheduled_drift_alerts_operator.rs::Watch::record` / `judge`, Python
  `support.py::scheduled_result`, TS schedule journey and
  `fixtures/cards/scheduled_drift_alerts_operator/{daily,latency-watch}.yaml`
  cover submission, due scheduling and retained results. The drift draft
  already teaches a Trigger/baseline path. Gap: the runnable guide/source and
  its clean setup/readback, not scheduler or baseline functionality.
- **Failure inspection:** Rust building-block journeys use
  `Execution::connect_with`, status/details and pinned evidence; Python/TS
  equivalents expose public `verify.Execution` and Bifrost inspection.
  `run-a-verification-and-inspect-its-evidence.md` is the draft procedure;
  `respond-to-a-failed-verification.md` retains response/Operator guidance.
  Gap: complete current source-backed instructions and captured negative
  evidence, not a new evidence API or durable store.
- **Setup/manifests:** All planned tests below use existing package/targets or
  SDK integration files. Rust `tests/integration/main.rs` registers their
  modules in one binary. Existing examples package has installed HTTP/runtime
  support. Keep fixture structure private, avoid a new test binary unless its
  real multi-crate/server purpose earns it, and add no dependencies.
- **Documentation consumers:** Existing `CodeFromFile`, `pageText`, content/nav
  and llms generators project the published pages. Source and proof changes
  must close embeds, frontend snippets and any example/test copy lists.
  Follow current `docs/AGENTS.md`: numbered guide steps with files, reader-native
  commands and captured output, then `If it fails` and `Next`. A conceptual
  timing explanation stays at its concept owner. Replace the old request
  draft's categorical advice to move model judges off-request: the current
  production practice permits them when measured time fits the decision. No
  hand edits of generated output or duplicate tutorial loader.

## Verification and Evidence

Use owning setup `mise run build:server`, `mise run py:setup`,
`mise run ts:build`, `mise run ts:build:testing`, and `mise run docs:install`
for the presented surfaces, sequentially. Rust package `wyrd-ai` has the
existing `integration` target; Python and TS files below already exist and
contain relevant local-provider/real-server fixtures. The wrapper provisions
Postgres and required migrations. It does not supply the model responses.

Apart from the explicitly existing baseline, test selectors below are
**planned**. Add them within the existing owner,
confirm actual nonempty collection before RED, and record any equivalent
selector/owner repair. Each must execute the tutorial's actual source, not
merely duplicate its leaf SDK calls. If existing behavior already fully passes,
record verification-only proof rather than manufacturing production churn.

### Live-request proof

Existing Python actual-handler regression (verification-only baseline; extend
this owner rather than add a second test for the same cases):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_docsite_quickstart.py::test_finance_api_releases_only_passed_reports"
```

Planned Rust, Python and TypeScript selectors for the remaining request proof,
respectively. Extend the Python baseline instead when that keeps the scenario
clearer; record the actual focused selector and do not duplicate its coverage:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verify_in_real_time::docsite_live_request_example)'"
```

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_verify_in_real_time.py::test_docsite_live_request_example"
```

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/verify-in-real-time.test.ts -t '^docsite_live_request_example$'"
```

### Workflow proof

Planned Rust, Python and TypeScript exact selectors, respectively:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verifier_building_blocks::docsite_agent_workflow_example)'"
```

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_verifier_building_blocks.py::test_docsite_agent_workflow_example"
```

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/verifier-building-blocks.test.ts -t '^docsite_agent_workflow_example$'"
```

### Schedule proof

Planned Rust, Python and TypeScript exact selectors, respectively:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=scheduled_drift_alerts_operator::docsite_scheduled_verification_example)'"
```

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_scheduled_drift_alerts_operator.py::test_docsite_scheduled_verification_example"
```

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/scheduled-drift-alerts-operator.test.ts -t '^docsite_scheduled_verification_example$'"
```

### Investigation proof

Planned Rust, Python and TypeScript exact selectors, respectively:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verifier_building_blocks::docsite_failure_investigation_example)'"
```

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_verifier_building_blocks.py::test_docsite_failure_investigation_example"
```

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/verifier-building-blocks.test.ts -t '^docsite_failure_investigation_example$'"
```

### Static and reader proof

No manufactured RED for guide prose, frontmatter or captured output. Regenerate
with `mise run docs:generate`, then:

```bash
mise exec -- pnpm --dir docs exec vitest run src/lib/indexes.test.ts src/lib/mdsvex/page-text.test.ts src/lib/content.test.ts
mise run docs:check:commands
mise run docs:linkcheck
mise run docs:svelte:check
mise run docs:build
mise run docs:a11y
```

Check that each rendered guide's files, commands, prerequisites and output
agree with its actual-program journey from a clean reader directory, including
its defining negative case. Record install pins for the release shipping this
change, provider/model setup, commands, actual output and source/journey links.
The continuous guide uses the public schedule and supported readback; scripted
local-provider results are labeled mechanics proof, not model-quality evidence.
The approved spec removed live-provider walkthrough and release-availability
gates: neither is a publication prerequisite. Actual-program real-server
journeys remain mandatory; a docs build alone is insufficient.

Run `mise run fmt` / `mise run lints`, `mise run py:format` /
`mise run py:lints` / `mise run py:typecheck`, and `mise run ts:format` /
`mise run ts:lints` / `mise run ts:typecheck` as applicable to changed source.
Finish with `mise exec -- git diff --check`. Broad SDK suites run once on the
integrated candidate. No backend hot-path change is authorized, so no hot-path
performance claim is established by these journeys.

## Material Stop Conditions

Return to spec for missing public evidence/completeness/scheduling behavior,
new dependency, runtime wait/write, security/tenancy or persistence changes.
Do not broaden runtime semantics to make a tutorial convenient. Correct private
selectors, small source organization and stronger equivalent proof locally;
never silently omit a required workflow or language variant.

## Authority Links

- [Approved specification, revision 5](../spec.md).
- [Repository rules](../../../../AGENTS.md), [workflow](../../../spec-driven-development.md),
  [execution](../../../implementation-execution.md), and [testing](../../../../TESTING.md).
- [Docs rules](../../../../docs/AGENTS.md), [editorial owner](../../../../docs/PRODUCT.md),
  and [contribution rules](../../../../docs/CONTRIBUTING.md).
- [Python rules](../../../../sdks/wyrd-sdk-python/AGENTS.md) and
  [TypeScript rules](../../../../sdks/wyrd-sdk-ts/AGENTS.md).
- [Wyrd Verifiers, authoring and execution](../../../../architecture/wyrd-design.md),
  [Bifrost evidence](../../../../architecture/bifrost-design.md), and
  [security boundaries](../../../../architecture/wyrd-security-posture.md).

## Implementation Evidence

Commits: `e118d1270` (request), `75e2d5360` (agent workflow), `d01c026f0`
(investigation), `75c1f91f7` (schedule).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Scenario 1: verdict before release; fail, inconclusive and typed error hold | `examples/finance-release/api.{py,rs,ts}`, `verify/verify-a-request-in-real-time.md` | Python `test_docsite_quickstart.py::test_finance_api_releases_only_passed_reports`; Rust `examples/tests/finance_release.rs::finance_api_releases_only_passed_reports`; TS `finance-release.test.ts` "the finance API releases only passed reports" | PASS |
| Scenario 2: complete passes, skipped fails, no trace inconclusive, from spans the program exported | `examples/refund-agent/refund_agent.{py,rs,ts}`, `verify/verify-an-agent-workflow.md` | Python `test_verifier_building_blocks.py::test_docsite_agent_workflow_example` (also asserts both stored span timelines); Rust `refund_agent.rs::refund_agent_fails_the_skipped_lookup`; TS `refund-agent.test.ts` | PASS |
| Scenario 3: the real every-minute scheduler judges a fast window `passed`, a slow one `failed`, an empty one `inconclusive`; results name the exact Verifier and Service | `examples/scheduled-check/{checkout/*.yaml,record.*,results.*}`, `verify/monitor-drift.md` | Python `test_scheduled_drift_alerts_operator.py::test_docsite_scheduled_verification_example` (217 s); Rust `examples/tests/scheduled_check.rs::scheduled_check_judges_each_window` (190 s); TS `scheduled-check.test.ts` (215 s). No harness due-time control is used | PASS |
| Scenario 4: stored failure found with bounded polling; failed requirement, expected and found values, exact versions; unknown and foreign-tenant reads are 404 | `examples/refund-agent/investigate.{py,rs,ts}`, `verify/run-a-verification-and-inspect-its-evidence.md` | Python `::test_docsite_investigate_example` (unknown and foreign tenant); Rust `refund_agent.rs` (unknown); TS `refund-agent.test.ts` (unknown) | PASS |
| Static and reader proof | Regenerated `llms.txt`, `llms-full.txt` | `docs:test` (102), `docs:check:commands`, `docs:linkcheck`, `docs:svelte:check`, `docs:build`, `docs:a11y` (42 pages), `check:examples`, `fmt`, `lints`, `py:format`, `py:lints`, `py:typecheck`, `ts:format`, `ts:lints`, `ts:typecheck`, `git diff --check` | PASS |

Commands: Python and TS journeys used `scripts/postgres/with-test-postgres.sh`
with `db:migrate:inner`; Rust used `db:migrate:all:inner` and
`cargo nextest run --locked -p wyrd-rust-examples -P journey --run-ignored=all`.

**Selector and owner repairs.** The planned `wyrd-ai --test integration`
selectors became `wyrd-rust-examples` tests, because those run the built example
binaries the guides show. The planned TS `verifier-building-blocks` and
`scheduled-drift-alerts-operator` selectors became the per-example files
`finance-release.test.ts`, `refund-agent.test.ts` and `scheduled-check.test.ts`,
following the existing pattern for docsite examples. The Python request proof
extends its existing baseline.

**Dependency judgment.** No new dependency was added. The examples manifests
gained workspace-locked packages already in the lockfiles: Rust
`opentelemetry` and `tracing-opentelemetry` (`workspace = true`, `Cargo.lock`
+2 lines, no new package) and TS `@opentelemetry/*` peers of the SDK's
telemetry (`pnpm install --offline` fetched 0 packages). They match the tracing
install the agent guide asks readers to run.

**Limits.** Journeys use synthetic domain data and no model provider, and make
no claim about model quality or latency. The scheduled journeys wait on real
minute boundaries, so they take about 3.5 minutes each. They also fail if the
host sleeps mid-run: the Postgres VM clock then jumps, the Forge lease is
refused, and Oracle readiness drops. In the CLI steps (`wyrd declare`,
`wyrd pull`, `create-principal`), the journeys drive the same server
operations through the SDK.

No runtime, schema, auth, migration or scheduler change was made.

### Handoff to Task 4

| Route | Title | Languages | Proof | Draft disposition |
|---|---|---|---|---|
| `/build-and-verify/verify/verify-a-request-in-real-time/` | Verify during a request | Python, Rust, TypeScript | Scenario 1 row | Draft completed at its URL |
| `/build-and-verify/verify/verify-an-agent-workflow/` | Verify an agent's workflow | Python, Rust, TypeScript | Scenario 2 row | New page; queries "agent skipped a step", "verify tool calls", "trace check" |
| `/build-and-verify/verify/monitor-drift/` | Check on a schedule | Python, Rust, TypeScript | Scenario 3 row | Draft rewritten around a Custom-metric schedule; the PSI/SPC method table and baseline readiness moved to `/reference/verifier-card/#checks`; untested drift snippets dropped |
| `/build-and-verify/verify/run-a-verification-and-inspect-its-evidence/` | Investigate a failed verification | Python, Rust, TypeScript | Scenario 4 row | Draft rewritten; `submit`/status semantics stay at `/understand/how-verification-works/`; binding readback is replaced by the account step on the schedule page |
| `/build-and-verify/verify/respond-to-a-failed-verification/` | Failures (Operators) | — | — | Remains a draft owned by `SPEC-docsite-incremental-reset`; its link to the removed binding step was repaired |

The overlap handoff is recorded in
`changes/active/docsite-incremental-reset/dispositions.md`.
