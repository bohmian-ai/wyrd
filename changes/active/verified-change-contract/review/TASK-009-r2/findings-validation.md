# TASK-009 R2 structured Ponytail validation

## Immutable subject and independent coverage

Fresh validator: `ponytail_rev`. Repository:
`/home/thorrester/Documents/GitHub/wyrd-verification-closeout`.
Base: `7d96c30066425e0cde2290842d5801307843283d`.
Candidate: `c47761decff8db95768d2c05f0b85b8ba62a021a`.

Inputs: approved `changes/active/verified-change-contract/spec.md` revision 45;
original `tasks/TASK-009-run-context-and-python-otel-correlation.md` and its
retained revision-35 requirement mappings; approved `architecture/logic/run_api.md`;
R1 verdict, validated ledger and `TASK-009-R1-restore-otel-correlation.md`;
all seven R2 discovery reports and `followup-review.md` in this directory.
No intended verdict was supplied. All required reports exist. HEAD matched the
candidate before and after inspection; reviewed source was unchanged. There is
no `.codegraph/` directory; repository search and direct source inspection were
used.

The cumulative diff and surrounding owners were inspected across shared
Run/state, PyO3 Run/state and observation siblings, Python OTel integration,
stub sources/public exports/assembler, Rust and TypeScript SDK projections,
changed tests/journeys, dependencies, and task/review evidence. Governing rules
were AGENTS.md, agent rules, spec-driven development, maintainer style, Rust,
Python/stubs, PyO3, telemetry and testing references, applicable Wyrd client and
observation-identity design, and the approved Run API. REQ-123/151, INV-007/012
and AC-032 remain present in the current approved specification.

## Producer, owner, and sibling-consumer tracing

`WyrdState::run_for_card` resolves through the hydrated `card_ref` index before
`Run::new` mints an invocation. `Run::for_card` preserves that invocation and
selects an immutable sibling subject. Python and TypeScript delegate to these
owners. Native explicit Drift, Eval and record paths obtain correlation from
this Run, not Python ambient state; Eval's Python active-span ID lookup is a
separate best-effort operation. Changing failure proof requires no producer or
observation implementation change.

`PyRun.__enter__` passes the native exact pair to `_enter_run`. That function
first calls `install_run_correlation`, whose global-provider path checks for
`add_span_processor`, tracks the provider in the weak registry, invokes the
registration method, removes failed bookkeeping, and returns False on failure.
Entry continues with the OTel private key, prior value, attach token and active
span. The processor reads that same key on every span start. `_exit_run`
consumes the local entry, attempts the exact detach, checks the resulting value,
and restores its recorded prior pair. Full bodies and callers of these owners,
the representative Drift helper, and affected tests were inspected. Existing
OtelObserver and explicit observation siblings need no correction.

The `Run.__exit__` signature is produced by the PyO3 attribute and method
parameters. Public `wyrd.observe.Run` imports that native class. Its stub is
produced separately from `python/wyrd/stubs/observe.pyi` by the existing
assembler. Ordinary `with` invokes the positional protocol; an explicitly
declared keyword call reaches PyO3 argument parsing before cleanup. Thus the
parity defect belongs at the public boundary sources, not in `_exit_run` or a
consumer wrapper. Existing testing context-manager signatures are independent
surfaces and do not justify changing them.

The two newly added Rust tests are reached by Rust's test harness. Both invoke
`state_fixture`, then use `expect`/`expect_err` and assertions on local selection
or identity. Their bodies and fixture, shared Run/state producers, and sibling
selection checks in Rust/Python/TypeScript were inspected. Panic paths are
intentional checks; only their required documentation is missing.

## Proposal decisions

### INV-REV-R2-001 / D-TEL-R2-001 / FOLLOWUP-009-1 — REVISED

Retain prior **FIND-TASK-009-1**, narrowed to actual processor-registration-
failure-to-explicit-observation proof. AC-032 at `spec.md:1825-1829` separately
requires API-only/no-SDK and processor registration failure. Scenario 3 agrees.
The remediation's compressed “API-only or registration failure” recommendation
cannot delete an approved obligation: spec-driven development forbids a derived
task weakening the approved spec.

At `test_observe_surface.py:375-388`, `Failing.add_span_processor` raises, but
only direct installation is called. At `:414-425`, Run/Drift proof uses
`object()`, so installation exits at `otel.py:260-262` before the registration
exception path at `:269-275`. The two green tests do not exercise the required
combined path. Production containment is presently supported by source; no
current escaping registration exception is alleged. The concrete gap is the
required regression check.

The behavior/system closure claims are rejected to this extent; invariant,
telemetry and follow-up claims are supported by the actual branches. The
remaining missing/API-only/attach/enrichment/detach, user-exception and unknown-
alias checks close the other portions of prior ID 1. No separate active-span
injection or per-observation matrix is needed.

### REPO-009-1 — CONFIRMED

Retain new **FIND-TASK-009-5**. At `src/observe/mod.rs:225-236`, the runtime
accepts `_exc_type=None`, `_exc_value=None`, `_traceback=None`. The owning
`stubs/observe.pyi:57-62` and generated `observe/__init__.pyi:59-64` instead
declare required `exc_type`, `exc_value`, `traceback`. Maintainer style's typed
contract explicitly requires runtime/public declaration agreement; AGENTS §8
and Python/stub rules require source-generated public typing.

Independent runtime inspection returned
`(self, /, _exc_type=None, _exc_value=None, _traceback=None)`. Using a real
offline public Run, `run.__exit__()` returned False, while
`run.__exit__(exc_type=None, exc_value=None, traceback=None)` raised
`TypeError: Run.__exit__() got an unexpected keyword argument 'exc_type'`.
The approved API and public stub name those conventional parameters. A direct
public call is reachable even though positional `with` hides the mismatch.
Template-to-generated parity does not prove parity with runtime.

Correct both public boundary sources to use `exc_type`, `exc_value`, `traceback`
with their existing runtime None defaults reflected in the owning stub;
regenerate through the assembler. Preserve precise exception/traceback types,
Literal[False], positional context-manager behavior, best-effort cleanup, and
exception propagation. Removing existing optional runtime calls or adding
aliases/wrappers is unnecessary. The existing signature and generator mechanisms
suffice; this aligns an approved API rather than introducing a new decision.

### REPO-009-2 — CONFIRMED

Retain new **FIND-TASK-009-6**. New tests at
`crates/shared/wyrd-client/src/observe/tests.rs:648-682` have summary rustdoc
but no `# Panics`. Both have reachable fixture/assertion panic paths.
AGENTS §16 explicitly includes test functions and requires `# Panics` whenever
a panic remains possible; agent rules and Rust documentation authority reinforce
this hard completion requirement. Older adjacent tests without the section do
not waive the rule for newly added tests.

Add concise panic-condition documentation to these two new tests only, naming
fixture loading and incorrect selection/invocation/refusal assertions. Existing
`state_fixture` documents loading panics, and the newly added Rust SDK
`assert_initial_card_selection` supplies an in-pattern selection/assertion
example. Preserve all assertions. No executable change, new test, blanket
documentation cleanup, or lint suppression is justified.

## Final deduplicated finding ledger

| Stable ID | Discovery source IDs | Status | Classification | Violated obligation | Exact location | Observable consequence | Smallest safe correction | Focused closure proof |
|---|---|---|---|---|---|---|---|---|
| FIND-TASK-009-1 | INV-REV-R2-001, D-TEL-R2-001, FOLLOWUP-009-1; prior INV-REV-001 | REVISED | MISSING | AC-032 and Scenario 3 require actual processor-registration-failure proof through Run entry and explicit observations. | `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:375-388,414-425`; actual branch `python/wyrd/otel.py:269-275` | A registration-to-observation coupling can regress while all required-looking focused tests pass; actual failure is tested only as installation. | Extend the existing failure-test home to select a global provider whose `add_span_processor` actually raises, enter a Run, and reuse the existing representative Drift boundary assertion. Establish the registration method was reached. Preserve API-only and other failure proof; change no production code or observation semantics. | Exact failure test and focused surface file show the injected actual registration attempt reaches `WYRD_SDK_400_BIFROST_NOT_STARTED`, with normal Run exit and unchanged user exceptions. |
| FIND-TASK-009-5 | REPO-009-1 | CONFIRMED | VIOLATION | Approved Run API and maintainer typed-contract/runtime-stub parity rules. | `sdks/wyrd-sdk-python/src/observe/mod.rs:225-236`; `python/wyrd/stubs/observe.pyi:57-62`; generated `python/wyrd/observe/__init__.pyi:59-64` | Type-accepted conventional keyword calls fail before cleanup; runtime-accepted omitted arguments are rejected by typing. | Align the existing PyO3 boundary to the approved conventional parameter names and reflect the existing None defaults in the owning stub. Regenerate public output with the current assembler. Preserve precise types, False result and cleanup; add no alias/wrapper or lifecycle work. | A public Run accepts conventional keyword and omitted-argument calls and returns False; inspect signature/default parity, retain positional `with` and user-exception proof, run `py:typecheck` and `codegen:check`. |
| FIND-TASK-009-6 | REPO-009-2 | CONFIRMED | VIOLATION | AGENTS §16 explicitly requires panic documentation for new Rust tests. | `crates/shared/wyrd-client/src/observe/tests.rs:648-682` | The new tests omit the hard-required account of fixture and assertion failure conditions. | Add brief `# Panics` sections to the two new selection/refusal tests only; keep executable bodies and assertions intact. | Source inspection and Rust formatting prove both sections accurately describe their unchanged panic paths; no additional runtime test is needed. |

## Prior-finding closure and correction assessment

| Prior ID | Assessment | Evidence |
|---|---|---|
| FIND-TASK-009-1 | PARTIALLY CLOSED; retained above | Missing package, API-only, attach, processor enrichment and detach now pair with Drift; actual registration failure does not. |
| FIND-TASK-009-2 | CLOSED | `observe/mod.rs:52-55` describes root, initially selected and sibling Cards, matching both constructors and immutable `for_card`. |
| FIND-TASK-009-3 | CLOSED | `otel.py:279-326` records prior values and restores after raising/swallowed detach; `test_observe_surface.py:448-467` proves outer restoration and post-exit no correlation in the same execution context. |
| FIND-TASK-009-4 | CLOSED | `otel.py:201-212` double-checks lazy creation under the existing lock; `test_observe_surface.py:470-512` deterministically overlaps first direct entries and proves one key and exact pairs. |

The Ponytail ladder removes no required correction: AC-032 explicitly requires
the missing case, the public signature actually rejects a declared call, and
§16 expressly requires the omitted documentation. Existing owners, test home,
generator, and rustdoc mechanism provide all three corrections. No dependency,
new harness, duplicated validation, consumer guard, broader refactor, lifecycle
change, or new material product/API/security/concurrency/persistent decision is
needed. New findings use the next unused IDs 5 and 6; closed IDs are not reused.

## Independent verification and result

From `sdks/wyrd-sdk-python`, independently ran:
`mise exec -- uv run python -m pytest -q tests/unit/state/test_observe_surface.py`:
**33 passed in 0.89s**. Runtime signature inspection and a real public Run call
independently reproduced the declaration mismatch above. The diagnostic call's
exit 1 was the expected TypeError evidence, not a passing check. Cumulative
`git diff --check` passed. Reviewed task/remediation and discovery reports supply
broader gate and real persisted-journey evidence; Cargo, codegen and Postgres
lanes were not rerun by this validator. Green checks do not fill the missing
scenario or erase source-proven contract/documentation violations.

All proposed findings are independently revised or confirmed; no unresolved
authority conflict or unavailable required report remains. This report does not
edit the reviewed implementation or write the orchestrator's verdict.

**Validation result: FIX_REQUIRED — three retained findings:
FIND-TASK-009-1, FIND-TASK-009-5, FIND-TASK-009-6.**
