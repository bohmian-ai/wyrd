# TASK-009 R2 focused follow-up

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `c47761decff8db95768d2c05f0b85b8ba62a021a`
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Remediation: `changes/active/verified-change-contract/review/TASK-009-r1/TASK-009-R1-restore-otel-correlation.md`
- Prior ledger: `changes/active/verified-change-contract/review/TASK-009-r1/findings-validation.md`

Fresh focused reviewer: `followup_rev`. This report resolves only the conflicting
closure claims for `FIND-TASK-009-1`. It is discovery evidence for the subsequent
independent validator, not a final validated finding ledger. HEAD matched the
candidate during inspection. No reviewed source was modified; there is no
`.codegraph/` directory, so source search and direct inspection were used.

## Paths inspected

The review read the original task, current approved specification (revision 45,
with TASK-009's revision-35 REQ-151 and AC-032 obligations retained), Run API,
R1 remediation and validated ledger, and R2 behavior, invariant, telemetry,
system, standards and maintainer reports. Applicable authority included
`AGENTS.md`, `architecture/agent-rules.md`,
`architecture/references/languages/spec-driven-development.md`, and
`architecture/references/languages/maintainer-style.md`.

Source investigation covered the cumulative changed Run/state and SDK
projections, stub sources/declarations, focused Python scope tests and extended
OTLP journey. For the disputed path it read complete bodies and callers of
`PyRun.__enter__`, `install_run_correlation`, `_enter_run`, `_exit_run`, and
`_drift_reaches_the_ordinary_boundary`, including the sibling native observation
surface and all focused optional-telemetry failure cases:

- `sdks/wyrd-sdk-python/src/observe/mod.rs:195-245`;
- `sdks/wyrd-sdk-python/python/wyrd/otel.py:186-326`;
- `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:375-467`;
- `changes/active/verified-change-contract/spec.md:315-339,1807-1833`;
- R1 remediation's recommendation 3 and acceptance criterion 3;
- R1 ledger's `INV-REV-001` and `FIND-TASK-009-1` entries.

## Authority resolves the conflict

AC-032 at `spec.md:1825-1829` explicitly enumerates missing API packages,
API-only/no-SDK provider, **processor registration failure**, span enrichment
failure, and detach failure. Focused Python tests must prove each does not
escape or block explicit observations. TASK-009 Scenario 3 also names both an
API-only/unsupported provider and registration failure.

The R1 recommendation says “API-only or registration failure”, whereas its
diagnosis refers to the approved registration-failure evidence and its
acceptance wording compresses the two into “API-only/registration”. That
compression explains the competing closure claims but cannot remove the
separate approved obligation. The spec-driven development authority orders the
approved specification above derived tasks and expressly says task planning
may not weaken or reinterpret it. No approved revision in the supplied subject
removes processor-registration-failure proof. This requires a bounded proof
correction under existing authority, not a new specification decision.

## Reachable path and evidence

`PyRun.__enter__` delegates to `_enter_run`. That function invokes
`install_run_correlation()` before context attach, causing global provider
lookup and optional processor registration. A provider implementing
`add_span_processor` can raise inside `otel.py:269-273`; installation removes
its registration bookkeeping and returns `False` through the outer handler.
Run entry continues, and explicit Drift uses the existing native Run rather
than reading this ambient telemetry state.

The current tests exercise two different paths:

| Test evidence | Executed path | Explicit observation proof |
|---|---|---|
| `test_unsupported_providers_are_refused_without_raising`, lines 375-388 | `Failing.add_span_processor` raises and direct installation returns `False` | None: no Run entry or emission occurs |
| `test_registration_and_attach_failures_never_block_observations`, lines 414-425 | Global provider is `object()`; installation returns at the missing-method branch, `otel.py:260-262`, before registration | Drift reaches its ordinary boundary, but actual registration never runs |

The test name does not establish registration-failure coverage. Other failure
cases and green tests cannot compose into the missing injected-failure-to-Run-
entry-to-observation check that AC-032 specifically requires. The system
report's claim that registration failure now reaches Drift overstates those
lines. Source supports production fail-open behavior today; this finding is a
required regression-proof gap, not a demonstrated current escaping exception.

## Proposed finding

**FOLLOWUP-009-1 — MISSING; retain `FIND-TASK-009-1` for validation.**

- Violated obligation: AC-032 and original Scenario 3's actual processor
  registration failure must not escape or block explicit observation emission.
- Location: `test_observe_surface.py:375-388,414-425`.
- Evidence: the raising registration provider is tested only through direct
  installation; the Run/Drift test uses a provider with no registration method.
- Consequence: the required regression check is absent, allowing later coupling
  between registration and explicit observation behavior to regress while the
  focused suite remains green.
- Smallest testable correction: extend the existing focused failure test home
  so Run entry selects a global provider whose `add_span_processor` actually
  raises, then reuse `_drift_reaches_the_ordinary_boundary` to prove explicit
  Drift reaches `WYRD_SDK_400_BIFROST_NOT_STARTED`. Preserve the existing
  API-only, attach, enrichment, detach, missing-package, unknown-alias and user-
  exception evidence. No production change, dependency, new harness or
  per-observation-method matrix is needed.
- Focused closure: from `sdks/wyrd-sdk-python`, run
  `mise exec -- uv run python -m pytest -q tests/unit/state/test_observe_surface.py`.

This is the remaining part of the prior proof finding, not a new finding ID or
an implementation redesign. Independent validation must determine its final
retention and correction.

## Other discovery claims and limits

The standards report's runtime/declaration and Rust test panic-documentation
claims identify separate concrete source locations. Neither creates another
authority conflict or unreviewed runtime path necessary to resolve this focused
question. They remain proposed findings for the required independent validator;
this follow-up does not validate or reject them.

No tests were rerun for this source-and-authority conflict. Existing discovery
reports supply the 33-test passing focused result; a passing suite does not
supply its missing scenario. No additional proposed finding arose.

**RESOLVED** — AC-032 retains separate processor-registration-failure proof;
the conflicting closure claims are resolved by approved authority and the
actual branch reached by each test.
