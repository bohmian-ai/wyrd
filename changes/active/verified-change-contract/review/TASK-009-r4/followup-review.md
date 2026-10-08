# TASK-009 round 4 focused follow-up — provider registration ambiguity

## Immutable subject and question

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `017a54d5390eb488e820f890bbb953a5f1ca3a53`
- Conflict: `INV-REV-R4-001` reports that a provider which retains the Wyrd
  processor and then raises can receive duplicates because the candidate removes
  its registration mark. The behavior, system, telemetry, and concurrency
  reviews accepted registration as idempotent.

The candidate remained `HEAD`, and the tracked `otel.py` blob remained exactly
the candidate blob (`da16b02bcb014e9654ba3d7a325f9a3b967a563a`) throughout this
follow-up. No source was modified.

## Source path inspected

The focused path was traced through:

- approved `REQ-151` and `AC-032` in
  `changes/active/verified-change-contract/spec.md:315-339,1825-1832`;
- TASK-009 Scenario 3 and its public hook in
  `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md:40-48,125-142`;
- the provider contract in
  `changes/active/verified-change-contract/architecture/logic/run_api.md:145-169,198-206`;
- the R1/R2 diagnoses and constraints in
  `changes/active/verified-change-contract/review/TASK-009-r1/TASK-009-R1-restore-otel-correlation.md`
  and
  `changes/active/verified-change-contract/review/TASK-009-r2/TASK-009-R2-close-proof-and-boundary-parity.md`;
- candidate registration and its Run-entry caller in
  `sdks/wyrd-sdk-python/python/wyrd/otel.py:246-288`;
- healthy, unsupported, and raising-provider proofs in
  `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:392-427,469-497`;
- the pinned dependency declaration in
  `sdks/wyrd-sdk-python/uv.lock:2856-2867`; and
- the installed official OpenTelemetry SDK 1.42.1 implementation at
  `sdks/wyrd-sdk-python/.venv/lib/python3.12/site-packages/opentelemetry/sdk/trace/__init__.py:166-169,1441-1449`.

`install_run_correlation` accepts any weak-referenceable object exposing a
callable `add_span_processor`. It marks that provider, invokes the foreign
method, then discards the mark on any exception. Every later Run entry calls
the helper again. The current raising-provider test records the supplied
processor before raising and expressly expects a second call, but it does not
retain the processor in a provider pipeline. The healthy test proves only the
ordinary success path.

## Resolution evidence

The stock OpenTelemetry 1.42.1 `TracerProvider.add_span_processor` delegates to
its active multi-processor. Both official multi-processor implementations append
the processor tuple while holding their lock and perform no later operation
that normally raises. A stock provider therefore does not itself expose the
reported retain-then-raise sequence.

That narrower stock behavior does not remove the reachable Wyrd path. The
approved escape hatch is explicitly for a framework-owned private provider,
and the Run API authority says Wyrd duck-types provider registration. The
candidate likewise recognizes support solely from `add_span_processor`; it
does not require the concrete stock class. A realistic framework wrapper can
delegate to the stock provider, thereby retaining the processor, and then fail
in its own post-registration hook. A source-free reproduction using exactly
that wrapper returned `(False, 1)` on the first installation and `(False, 2)`
on the second, counting Wyrd processors retained by its inner stock provider.
Thus the failure is not dependent on an impossible stock SDK state or a
test-only inaccessible object.

The approved behavior is also unambiguous at this boundary:

- REQ-151 requires one idempotently registered processor and an idempotent
  private-provider hook.
- AC-032 requires provider registration to be idempotent.
- TASK-009 Scenario 3 strengthens this to global and explicit private providers
  receiving **at most one** Wyrd processor each, while separately requiring a
  registration failure to fail open.
- Scenario 3's refactor instruction says to delete retries not required by the
  behavior. R1 and R2 likewise preserve fail-open behavior without adding a
  telemetry retry mechanism.

Those obligations do not condition idempotency on a successful return from an
untrusted foreign method. Once that method raises, Wyrd cannot establish that
the side effect did not occur. Clearing the mark and retrying therefore
contradicts the explicit at-most-one rule. Fail-open requires the error not to
escape or block application/observation behavior; it does not require another
registration attempt.

The behavior/system/telemetry/concurrency PASS claims are correct for the
stock-success and raises-before-retention paths they inspected, but they do not
cover this allowed private-provider path. Their conclusions therefore do not
invalidate `INV-REV-R4-001`.

## Finding impact

No new proposed finding is needed. The conflict resolves in favor of retaining
`INV-REV-R4-001` as proposed for independent validation. Its correction remains
bounded to the existing locked, weak per-provider registration bookkeeping:
after an ambiguous failed attempt, later calls must remain fail-open and report
unavailable without handing the provider another Wyrd processor; successful
providers must continue returning success idempotently. The focused proof must
use a retaining-then-raising provider and cover a later call or Run entry plus
the unchanged explicit-observation boundary.

## Result

**RESOLVED** — the stock provider alone does not retain and then raise, but the
approved and implemented duck-typed private-provider boundary admits a
realistic wrapper that does. The unqualified idempotency and at-most-one
obligations make this required behavior, not speculative hardening.
