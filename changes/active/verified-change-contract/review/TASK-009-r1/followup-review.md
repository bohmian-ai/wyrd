# TASK-009 Focused Follow-up Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `e4f30547906b8046bdccbfd38dff0eb0ee475d06`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 35
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Uncertainty reviewed: whether `INV-REV-001` identifies a material missing
  proof despite the behavior review's acceptance of the combined source and
  test evidence.

## Source paths inspected

- `changes/active/verified-change-contract/spec.md:315-338,1807-1832`
- `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md:125-142,182-193`
- `changes/active/verified-change-contract/architecture/logic/run_api.md:134-206`
- `sdks/wyrd-sdk-python/python/wyrd/otel.py:186-307`
- `sdks/wyrd-sdk-python/src/observe/mod.rs:125-153,195-245,270-337`
- `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:352-428`
- `sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py:345-474,517-541`
- Complete base-to-candidate diff for the paths above, plus repository-wide
  searches for the injected failure messages and explicit `run.observe.*`
  calls.

## Resolution evidence

The proof is materially required. `AC-032` does not merely require each
telemetry failure to be swallowed independently. It says focused Python tests
**must prove** that a missing package, API-only/no-SDK provider, processor
registration failure, span-enrichment failure, and detach failure "do not
escape or block explicit observations" (`spec.md:1825-1828`). TASK-009 repeats
the same coupled outcome in Scenario 3 (`TASK-009...md:125-139`). Passing an
installation helper test beside unrelated observation tests does not exercise
that required failure-to-observation path.

The current focused coverage proves only part of that matrix:

- `test_missing_opentelemetry_is_a_no_op` enters a Run with the modules absent
  and calls `entered.observe.drift`, reaching the ordinary
  `WYRD_SDK_400_BIFROST_NOT_STARTED` result (`test_observe_surface.py:390-398`).
  This is direct proof that the missing-package failure does not block an
  explicit observation.
- `test_unsupported_providers_are_refused_without_raising` injects API-only and
  registration-failing providers but calls only `install_run_correlation`
  (`test_observe_surface.py:374-387`). It never enters a Run or emits an
  observation.
- `test_enrichment_and_detach_failures_never_escape` injects processor context
  lookup and detach failures and verifies an uncorrelated span, user-exception
  propagation, and the `False` exit result (`test_observe_surface.py:401-428`).
  It never calls `run.observe.*` while either failure is active.
- No other Python test combines those injected registration, enrichment, or
  detach failures with an explicit observation. The integration journey proves
  healthy telemetry plus real observations; it does not exercise these
  injected failures.

Source inspection supports the intended behavior but does not satisfy the
specified proof obligation. Registration, processor enrichment, attach, and
detach are caught in `otel.py:217-226,241-307`; Drift and record emission call
the native observation owner independently at `src/observe/mod.rs:270-283` and
`325-337`. Eval additionally performs its own best-effort active-span lookup at
`src/observe/mod.rs:125-153,285-323`. This makes the implementation plausibly
correct today, but AC-032 explicitly selected a focused behavioral test as the
required evidence class.

`REQ-151` separately names attach failure as fail-open
(`spec.md:327-331`). AC-032's explicit focused-test list does not separately
name attach failure, so `INV-REV-001` should not rely on claiming that AC-032
itself enumerates it. The task still requires the behavior, and no current test
injects `opentelemetry.context.attach` failure. Adding that case beside the
other Scenario 3 injections is the smallest credible direct proof, but the
finding remains material even if validation limits its strict AC-032 wording
to registration, enrichment, and detach.

The smallest correction is test-only and uses the existing server-free
fixture: drive each missing registration/enrichment/detach pairing through
`with run` and one representative explicit observation, asserting its normal
boundary result rather than the injected telemetry exception; add the absent
attach-failure injection for the REQ-151 branch. No new harness, production
guard, abstraction, or test of every observation method is needed.

## Proposed findings

### INV-REV-001 — CONFIRMED WITH NARROWED AUTHORITY WORDING

- **Classification:** MISSING
- **Violated obligation:** AC-032 and TASK-009 Scenario 3 require focused
  failure-injection proof that registration, span-enrichment, and detach
  failures do not block explicit observations. REQ-151 also requires attach
  failure to fail open.
- **Location:**
  `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:374-424`.
- **Evidence:** only the missing-package case reaches `run.observe.*`; the
  registration, enrichment, and detach injections do not, and attach failure
  is not injected.
- **Observable consequence:** the candidate lacks the expressly required
  regression proof that optional telemetry failures remain isolated from the
  explicit observation surface, although the current source is structured to
  provide that isolation.
- **Testable correction:** extend the existing Scenario 3 tests to pair the
  required injected failures with one explicit observation reaching its normal
  server-free boundary result, including one attach-failure injection. Preserve
  the existing no-escape, user-exception, and unknown-alias assertions.

No new proposed findings.

## Result

**RESOLVED**

The disagreement is evidentiary rather than an implementation-path conflict.
The source supports fail-open behavior, but the approved acceptance criterion
requires direct focused proof that is missing for registration, enrichment,
and detach failures. `INV-REV-001` is retained with the authority wording above.
