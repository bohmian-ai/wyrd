---
id: TASK-AUDIT-OUTBOX-R4-CLOSURE
kind: remediation
status: review
spec: SPEC-audit-outbox
spec_revision: 4
requirements: [REQ-003, REQ-003a, REQ-008, AC-008]
parent_task: TASK-AUDIT-OUTBOX-R3-COMMIT-OUTCOME
remediates: [FIND-AUDIT-OUTBOX-12, FIND-AUDIT-OUTBOX-7, FIND-AUDIT-OUTBOX-15]
route_to: wyrd-implement
---

# Close the audit-outbox r3 closure findings

## Authority and immutable input

- Approved specification: `changes/active/audit-outbox/spec.md`, revision 4.
- Original implementation tasks:
  `changes/active/audit-outbox/tasks/01-publication-progress.md`,
  `02-one-outbox.md`, and `03-remove-audit-unavailable.md`.
- Prior remediation tasks:
  `changes/active/audit-outbox/review/r2/TASK-AUDIT-OUTBOX-R2-bounded-remediation.md`
  and `TASK-AUDIT-OUTBOX-R3-commit-outcome.md`.
- Closure verdict and validated ledger:
  `changes/active/audit-outbox/review/r3/verdict.md` and
  `findings-validation.md`.
- Review base: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8`.
- Reviewed candidate: `52e1144b5c186ccacd85d9c779a4e60c3cce5ac2`.

Use `$wyrd-implement`. Do not edit the approved specification.

## Issue diagnoses

### FIND-AUDIT-OUTBOX-12 — error rendering escapes panic containment

The generic writer correctly contains panics raised while constructing or
polling `OutboxSink::write`, retaining the sole item vector for the existing
front-of-queue retry. It then converts `S::Error: Display` with
`error.to_string()` outside that containment at
`crates/shared/wyrd-runtime/src/outbox.rs:328-342`. A valid sink error whose
`Display` panics unwinds the child before it returns `(tenant, items, result)`.
The parent `JoinError` path at `outbox.rs:382-388` owns only tenant and count,
so it counts/releases the batch as lost while the process and writer continue.
That violates REQ-003, REQ-003a, REQ-008, and AC-008: accepted loss is limited
to abrupt stop, shutdown deadline, and revision-4's explicit unresolvable
transaction-status terminal. Existing tests cover an async-body panic but not
this post-containment path.

### FIND-AUDIT-OUTBOX-7 — the closure journey repeats the import/type rule

The new AC-009 journey at
`crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:839-841,915-921`
declares `CommitCutter::addr` and `relay` parameters/return through fully
qualified standard-library and Tokio types, and imports Tokio IO extension
traits inside `relay`. This repeats the exact hard dependency-manifest rule the
candidate was meant to close: declarations use imported bare names and all
ordinary imports live at module scope. The behavior is correct, but the proof
code remains non-compliant.

### FIND-AUDIT-OUTBOX-15 — required rustdoc is absent on changed test methods

`MemorySink::write` at
`crates/shared/wyrd-runtime/src/outbox.rs:519-543` was materially changed to
signal dispatch, await an injected hang, consume a one-shot panic injection,
then fail or record the batch. The six new methods in `Recorder for TestMetrics`
at `outbox.rs:614-629` also lack rustdoc, including the intentional no-op
descriptor and histogram behavior. `AGENTS.md` section 16 requires substantive
rustdoc on every new or materially modified Rust item, including private test
helpers and trait implementation methods. Without it, maintainers can reorder
the injection stages or expand the metric model and silently weaken the proof.

## Intended correction outcome

Every ordinary sink-controlled panic before the child returns its owned batch
is converted into the existing failed-write outcome, so the original items
remain pending and retry once in order. The AC-009 journey exposes all
dependencies through its module import block and uses bare declaration types.
The changed outbox test methods document the behavior and invariants their tests
depend on. No audit contract, persistence shape, transaction-outcome behavior,
or public surface changes.

## Decision-complete correction

1. Keep the item vector owned by the existing child task and extend the current
   unwind containment only far enough to include sink-error normalization.
   A panic from rendering an ordinary sink error must become the same contained
   failed-write result used by construction and polling panics, so the existing
   `finish` path restores the batch to the front of its tenant queue, applies
   backoff, and increments the write-failure metric. Reuse the installed
   `catch_unwind`, current writer, and current retry path. Do not clone items,
   add a queue, create a general panic abstraction, or attempt to guarantee
   behavior for adversarial panicking destructors.
2. Extend the existing module import block in the ambiguous-commit journey with
   the network, IO-result, TCP stream, and Tokio IO extension-trait names it
   uses. Choose an unambiguous alias for the IO result, use bare names in the
   field and function signature, and remove the function-scoped import. Keep
   the proxy and journey behavior otherwise unchanged.
3. Add substantive rustdoc only to the materially changed `MemorySink::write`
   and the six new recorder methods. For the sink, state the signal, hang,
   one-shot panic, failure, and successful-write order; document `# Errors`,
   `# Panics`, and cancellation behavior. For each recorder method, state
   whether it records by metric name or intentionally ignores the operation.
   Do not document unrelated untouched code or add a documentation check.

The source of the invalid panic state is the child task's uncontained error
rendering; correct it once at that owner rather than adding a downstream guard
to the `JoinError` branch, which no longer has the items. The other two findings
are local proof-code conformance fixes and require no abstraction or behavioral
redesign.

## Constraints and preserved behavior

- Preserve revision-4 commit-outcome resolution exactly: committed succeeds,
  aborted retries, unresolved waits without re-send, and unavailable status is
  counted lost without re-send.
- Preserve one canonical staging append, one publisher, tenant binding,
  per-tenant ordering, four audit writer slots, shutdown admission fencing, and
  terminal pending/gauge accounting.
- Preserve the current AC-009 proxy injection and production writer/publisher
  journey; change only its imports and declaration spelling.
- Preserve the unwrap-audit allowlist correction and security-posture wording.
- Do not restore event IDs, the deleted unreleased migration, reader
  deduplication, another table, WAL, relay, publisher, or retirement delay.
- Do not add direct in-progress/unreachable or `NULL` branch tests in this task;
  their absence is an accepted verification limit, not a retained finding.
- `FIND-AUDIT-OUTBOX-5`, `mise run bench:capacity`, and `mise run gate` remain
  deferred to integration.

## Explicit non-goals

- General panic-safety machinery or support for adversarial destructors.
- Changes to production audit SQL, transaction identity, publication,
  retention, migration history, metrics, or public contracts.
- Refactoring or generalizing the test TCP proxy.
- New repository checks, lint allowances, ignored tests, or weakened gates.

## Acceptance criteria and proof

### AC-R4-1 — `FIND-AUDIT-OUTBOX-12`

A sink that returns an error whose `Display` panics once cannot escape with its
batch. The writer remains alive; the original batch commits exactly once ahead
of a later same-tenant item; the write-failure counter increments once; the
loss counter remains zero; and pending plus the pending gauge settle to zero.
Name the focused scenario
`a_panicking_error_display_is_retried_once_in_order_without_loss` and run:

```bash
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib \
  -E 'test(=outbox::tests::a_panicking_error_display_is_retried_once_in_order_without_loss)'
```

Then re-run every generic outbox scenario:

```bash
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib \
  -E 'test(/^outbox::tests::/)'
```

### AC-R4-2 — `FIND-AUDIT-OUTBOX-7`

The added proxy has no function-scoped import and no fully qualified type in a
field or function signature. Formatting and lints pass, and the existing
ambiguous-commit production journey still proves exactly-once retention:

```bash
scripts/postgres/with-test-postgres.sh -- mise exec -- \
  cargo nextest run --locked -p wyrd-testing --test server -P journey \
  --run-ignored=all \
  -E 'test(=audit_publication::ambiguous_audit_commits_retain_each_decision_exactly_once)'
```

### AC-R4-3 — `FIND-AUDIT-OUTBOX-15`

The changed sink method and all six new recorder methods carry substantive
rustdoc matching their behavior; the sink documents its failure, panic, and
cancellation contracts. Focused source inspection, formatting, lints, and the
full outbox test expression above pass.

## Broader verification

Run the smallest owning lanes after the focused proofs:

```bash
mise run fmt
mise run lints
mise run test:shared
mise run test:bifrost:journey:server
git diff --check
```

Record RED for the new panicking-`Display` scenario, GREEN after the owner fix,
the unchanged prior panic/shutdown scenarios, the exact journey result, and
the broader lanes in this task's evidence section. A Docker or other external
environment failure is a blocker to diagnose, not permission to weaken or skip
the required journey.

## Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-R4-1 (`FIND-AUDIT-OUTBOX-12`) | `crates/shared/wyrd-runtime/src/outbox.rs` `OutboxWriter::dispatch`: sink-error rendering moved inside `catch_unwind` in the child task; a rendering panic becomes the existing failed-write result, so `finish` restores the batch to the front, backs off, and counts one write failure. Test sink error type `MemoryError` with a panicking `Display` and one-shot `display_panicking` injection. | RED before fix: `a_panicking_error_display_is_retried_once_in_order_without_loss` failed with `the failed batch stays pending` (left 1, right 2), meaning the batch was lost. GREEN after fix: 1/1 passed. `test(/^outbox::tests::/)`: 8/8 passed, including the unchanged `a_panicking_write_...` and both shutdown scenarios. | PASS |
| AC-R4-2 (`FIND-AUDIT-OUTBOX-7`) | `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs`: module imports now include `IoResult` (alias of `std::io::Result`), `SocketAddr`, `TcpStream`, `AsyncReadExt`, `AsyncWriteExt`. `CommitCutter::addr` and the `relay` signature use bare names, and the function-scoped import is gone. | Exact journey `ambiguous_audit_commits_retain_each_decision_exactly_once`: 1/1 passed (31.2s). `mise run fmt` and `mise run lints` passed. | PASS |
| AC-R4-3 (`FIND-AUDIT-OUTBOX-15`) | rustdoc on `MemorySink::write` covering the injection order, `# Errors`, `# Panics` and `# Cancellation`, and on all six `Recorder for TestMetrics` methods, each saying whether it records by name or does nothing on purpose. The new test-only `MemoryError` and its `fmt` are documented too. | Source inspection; fmt, lints, and 8/8 outbox tests passed. | PASS |

Commands run (all exit 0):

- `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=outbox::tests::a_panicking_error_display_is_retried_once_in_order_without_loss)'`: RED before the fix, then 1/1 passed.
- `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(/^outbox::tests::/)'`: 8/8 passed.
- `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=audit_publication::ambiguous_audit_commits_retain_each_decision_exactly_once)'`: 1/1 passed.
- `mise run fmt`: clean. `mise run lints`: clean.
- `mise run test:shared`: 713 passed, 17 skipped.
- `mise run test:bifrost:journey:server`: 31/31 passed.
- `git diff --check`: clean.

Non-goals stayed out of scope: no production audit SQL, metrics, contracts, or proxy behavior changed, and no panic abstraction, lint allowance, or check was added. Expression-level fully qualified paths elsewhere in the journey file predate this change, and the finding did not cite them, so they were left as they are.
